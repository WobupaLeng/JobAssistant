"""Boss 直聘抓取器。

沿用原 Java 项目的做法：拦截 zhipin.com 的搜索接口 JSON，再逐个拉详情接口
取 JD 全文 + 公司侧信息（规模/融资/行业/HR 活跃度），这些正是打分最需要的字段。
"""
from __future__ import annotations

import json
import time
import re
from datetime import datetime
from typing import Any
from urllib.parse import quote

from .base import BaseScraper

CITY_CODE = {"上海": "101020100"}
SEARCH_BASE = "https://www.zhipin.com/web/geek/job"
DETAIL_API = "https://www.zhipin.com/wapi/zpgeek/job/detail.json?securityId={sid}&jobId={jid}"


class BossScraper(BaseScraper):
    platform = "boss"

    def read_expectations(self, ctx: Any) -> list[dict[str, Any]]:
        """读取 Boss 账号里配置的期望职位（我的简历 → 求职期望）。

        接口：GET /wapi/zpgeek/pc/recommend/expect/list.json
        要点（踩过的坑）：
        - 用 page.request（Playwright 的 APIRequestContext）直接请求，**不导航页面**：
          导航 zhipin 页面会被反爬踢成 about:blank / 卡在"加载中"
        - 复用已有标签页，没有才新建，结束后只关自己新建的那个
        """
        out: list[dict[str, Any]] = []
        url = "https://www.zhipin.com/wapi/zpgeek/pc/recommend/expect/list.json"
        page = None
        created = False
        try:
            for pg in ctx.pages:
                try:
                    if "zhipin.com" in (pg.url or ""):
                        page = pg
                        break
                except Exception:
                    continue
            if page is None:
                page = ctx.new_page()
                created = True

            txt = ""
            try:
                resp = page.request.get(url, timeout=20000)
                if resp.ok:
                    txt = resp.text()
                else:
                    self.warn(f"期望职位接口返回 HTTP {resp.status}")
            except Exception as exc:
                self.warn(f"期望职位接口请求失败：{exc}")

            if txt:
                data = json.loads(txt)
                for e in (data.get("zpData") or {}).get("expectList") or []:
                    name = str(e.get("positionName") or "").strip()
                    if not name:
                        continue
                    out.append({
                        "name": name,
                        "code": str(e.get("position") or ""),
                        "city": e.get("locationName") or "",
                        "city_code": str(e.get("location") or ""),
                        "salary": e.get("salary") or "",
                        "source": "boss_expect",
                    })
        except Exception as exc:
            self.warn(f"读取 Boss 期望职位失败：{exc}")
        finally:
            if created and page is not None:
                try:
                    page.close()
                except Exception:
                    pass
        return out

    def run(self, page: Any, on_job: Any = None) -> list[dict[str, Any]]:
        cfg = self.cfg
        jobs: dict[str, dict[str, Any]] = {}

        # 多关键词：Boss 的 query 参数只支持一个，所以按关键词逐个搜
        keywords = cfg.keyword_list() or [""]
        # 记录每个岗位命中的关键词，入库时写进 matched_keyword
        hit_kw: dict[str, str] = {}

        target = int(getattr(cfg, "target_count", 0) or 0)
        processed: set[str] = set()   # 已经拉过详情/提交过滤的岗位，多轮之间去重
        round_no = 0

        while not self.should_stop():
            round_no += 1
            if round_no > 1:
                self.info(f"===== 第 {round_no} 轮：重新搜索补充岗位（已入库 {self.arrived}"
                          + (f" / 目标 {target}" if target else "") + "）=====")
            else:
                self.info(f"关键词「{'、'.join(keywords) or '(空)'}」开始，URL 筛选：{describe_filter(cfg)}")

            before = len(jobs)
            for kw in keywords:
                if self.should_stop():
                    break
                self._search_keyword(page, kw, jobs, hit_kw)

            for jid, kw in hit_kw.items():
                if jid in jobs:
                    jobs[jid]["matched_keyword"] = kw

            # 只处理本轮新拿到的岗位；多轮之间不重复拉详情
            fresh = [(jid, it) for jid, it in jobs.items() if jid not in processed]
            if not fresh:
                self.info("这一轮没有新的岗位了，结束")
                break
            self.info(f"本轮新增 {len(fresh)} 个岗位，开始拉取详情…")

            for idx, (jid, item) in enumerate(fresh, 1):
                if self.should_stop():
                    break
                self.stop.wait_if_paused()
                processed.add(jid)
                self._fetch_detail(page, jid, item)
                if idx % 10 == 0:
                    self.info(f"详情进度 {idx}/{len(fresh)}")
                # 流式入库：这条 JD 已经解析完，马上交给 main 侧过滤/打分/入库
                if on_job is not None:
                    try:
                        reached = bool(on_job(item))
                    except Exception as exc:
                        self.warn(f"入库回调失败：{exc}")
                        reached = False
                    if reached:
                        self.info(f"已达标：入库 {self.arrived} / {target} 条，停止抓取")
                        return list(jobs.values())
                self.stop.sleep(0.3)

            if self.should_stop():
                break
            # 目标没达成：刷新后继续下一轮（Boss 会推荐新岗位）
            if target > 0 and self.arrived < target:
                self.info(f"本轮结束，入库 {self.arrived} / {target} 条，刷新页面后继续搜索…")
                try:
                    page.reload(wait_until="domcontentloaded", timeout=30000)
                    self.stop.sleep(2.5)   # 可中断等待
                except Exception as exc:
                    self.warn(f"刷新页面失败：{exc}")
                self.stop.sleep(2.0)
                continue
            break

        return list(jobs.values())

    def _search_keyword(self, page: Any, kw: str, jobs: dict[str, dict[str, Any]],
                        hit_kw: dict[str, str]) -> None:
        cfg = self.cfg
        for page_no in range(1, cfg.max_pages + 1):
            if self.should_stop():
                break

            captured: list[str] = []

            first_seen: list[Any] = []
            seen_resps: set[int] = set()   # 重试时同一响应只算一次

            def on_response(resp: Any) -> None:
                # 搜索接口：/wapi/zpgeek/search/joblist.json
                if resp in seen_resps:
                    return
                if "joblist.json" in resp.url and resp.status == 200:
                    seen_resps.add(resp)
                    try:
                        captured.append(resp.text())
                    except Exception:
                        # 偶发 "No resource with given identifier"：同步回调节点上
                        # 拿不到 body，先记下 resp，导航结束后再重试取一次
                        first_seen.append(resp)

            # 预热：先回首页再带筛选条件进搜索页。
            # 直接带 salary/experience 多参数跳转会触发 Boss 的"加载中/风控"页，
            # 页面根本不会去请求 joblist.json（实测）。
            try:
                page.goto("https://www.zhipin.com/web/geek/job", wait_until="domcontentloaded", timeout=30000)
                self.stop.sleep(1.2)
            except Exception:
                pass

            url = build_search_url(cfg, kw)
            self.info(f"关键词「{kw}」第 {page_no} 页")
            # 空页（风控验证码页 / 加载中 / 未登录跳转）不要空等，先看清页面是什么，
            # 能重试就重试（刷新 + 换个入口重进），重试几次还空才算这一页失败。
            captured.clear()
            first_seen.clear()
            page.on("response", on_response)
            empty_retry = 0
            try:
                page.goto(url, wait_until="domcontentloaded", timeout=30000)
                self.stop.sleep(3.5)   # 可中断等待（停止/暂停立即生效）
            except Exception as exc:
                self.warn(f"打开搜索页失败：{exc}")
                page.remove_listener("response", on_response)
                continue
            page.remove_listener("response", on_response)

            # 导航期间没取到 body 的响应，这里补取一次
            for resp in first_seen:
                try:
                    body = resp.text()
                except Exception:
                    continue
                if body:
                    captured.append(body)

            def _harvest_dom() -> int:
                dom_items = self._parse_dom_cards(page)
                added_dom = 0
                for item in dom_items:
                    jid = item.get("job_id")
                    if not jid or jid in jobs:
                        continue
                    jobs[jid] = item
                    hit_kw.setdefault(jid, kw)
                    added_dom += 1
                return added_dom

            if not captured:
                added_dom = _harvest_dom()
                if added_dom:
                    self.warn(f"第 {page_no} 页接口未捕获，改用页面卡片解析，拿到 {added_dom} 条")
            else:
                added_dom = 0

            # 空页处理：诊断 + 重试
            while not captured and not added_dom and empty_retry < 2 and not self.should_stop():
                empty_retry += 1
                reason = self._diagnose_empty_page(page)
                self.warn(f"第 {page_no} 页未拿到岗位：{reason}（第 {empty_retry} 次重试）")
                try:
                    # 退一步：先回首页去掉全部筛选参数，再带着条件重进
                    page.goto("https://www.zhipin.com/web/geek/job", wait_until="domcontentloaded", timeout=30000)
                    self.stop.sleep(2.0)
                    page.goto(url, wait_until="domcontentloaded", timeout=30000)
                    self.stop.sleep(3.5)
                except Exception as exc:
                    self.warn(f"重试打开搜索页失败：{exc}")
                # 重新挂响应监听后再刷新一次（刷新会重新发 joblist.json）
                page.on("response", on_response)
                try:
                    page.reload(wait_until="domcontentloaded", timeout=30000)
                    self.stop.sleep(3.0)
                except Exception as exc:
                    self.warn(f"重试刷新失败：{exc}")
                page.remove_listener("response", on_response)
                for resp in first_seen:
                    try:
                        body = resp.text()
                    except Exception:
                        continue
                    if body:
                        captured.append(body)
                first_seen.clear()
                if not captured:
                    added_dom = _harvest_dom()

            if not captured and not added_dom:
                self.warn(
                    f"第 {page_no} 页没有拿到任何岗位（接口与 DOM 都为空），"
                    f"可能触发风控或未登录。当前地址：{page.url}"
                )
                # 这一页彻底没戏：给用户一个明确动作，别让后面每页都白跑
                self.warn("建议：先在左栏点「登录」完成扫码/验证码，再重新开始抓取")
                if empty_retry >= 2:
                    self.info(f"关键词「{kw}」连续空页，结束该关键词")
                    break

            added = 0
            for raw in captured:
                for item in self._parse_search(raw):
                    jid = item.get("job_id")
                    if not jid or jid in jobs:
                        continue
                    jobs[jid] = item
                    hit_kw.setdefault(jid, kw)
                    added += 1
            self.info(f"第 {page_no} 页新增 {added} 条（累计 {len(jobs)}）")

            # 停止条件：用户点了停止 / 候选池满 / 已入库数达到目标（入库数由 main 侧推进）
            target = int(getattr(cfg, "target_count", 0) or 0)
            reached = target > 0 and self.arrived >= target
            # 候选池上限放宽到 target*3（默认 100 条），避免过滤损耗大时一轮就耗尽
            pool_cap = max(cfg.daily_limit or 0, min((target or 100) * 3, 1000))
            if self.should_stop() or len(jobs) >= pool_cap or reached:
                if reached:
                    self.info(f"已达标：入库 {self.arrived} / {target} 条，停止翻页")
                break
            # 翻页：点「下一页」
            try:
                self.jitter()  # 翻页前随机等待 1500~9800ms
                nxt = page.locator("a.ui-icon-arrow-right, .options-pages a:last-child").first
                if nxt.count() == 0:
                    self.info("没有下一页，结束")
                    break
                nxt.click(timeout=5000)
                self.stop.sleep(3.0)   # 可中断等待
            except Exception:
                self.info("翻页失败或已到末页，结束")
                break
            self.jitter()  # 每页之间的额外随机间隔

    def _diagnose_empty_page(self, page: Any) -> str:
        """空页到底是哪种：验证码/风控、登录失效、加载中、还是真没有结果。

        这段判断直接决定用户下一步该干什么，所以把页面特征翻成人话，
        别只丢一句「可能触发风控或未登录」。
        """
        try:
            cur = (page.url or "").lower()
            if "verify" in cur or "captcha" in cur:
                return "跳到了安全验证页（需要手动过验证码）"
            title = ""
            try:
                title = (page.title() or "")[:80]
            except Exception:
                pass
            if "安全验证" in title or "验证码" in title:
                return f"安全验证页（{title}）"
            text = self._safe_inner_text(page, "body", 3000)
            low = text.lower()
            for marker in ("安全验证", "请完成验证", "拖动滑块", "点击验证", "人机验证"):
                if marker in text:
                    return f"页面要求安全验证（{marker}）"
            if "/web/user/" in cur or "login" in cur:
                return "被重定向到登录页（登录态失效）"
            if "加载中" in text or "loading" in low:
                return "页面一直停在加载中"
            if "没有找到" in text or "暂无" in text or "换个条件" in text:
                return "筛选条件下 Boss 没有返回结果"
            if not text.strip():
                return "页面内容为空（可能被风控拦截）"
            # 有正文却没卡片：把「实际看到什么」说清楚，否则用户只会看到
            # 「网页明明有数据」和程序各说各话。带上链接数便于判断是不是选择器失配。
            links = 0
            for sel in ("a[href*='/job_detail/']", "a[href*='job_detail']"):
                try:
                    links = max(links, page.locator(sel).count())
                except Exception:
                    pass
            # 有链接就可能还能解析：先试一次，能解析出来就不是「结构变了」
            if links and self._card_container(page):
                return "卡片找到了但没解析出有效字段，正在重试"
            if links:
                return (
                    f"页面里有 {links} 个岗位链接，但卡片选择器一个都没匹配上"
                    f"（标题「{title}」，页面结构可能变了）"
                )
            snippet = " ".join(text.split())[:60]
            return f"页面标题「{title}」没有任何岗位卡片（正文开头：{snippet}）"
        except Exception as exc:
            return f"无法判断空页原因（{exc}）"

    @staticmethod
    def _safe_inner_text(page: Any, selector: str, limit: int) -> str:
        try:
            loc = page.locator(selector)
            if loc.count() == 0:
                return ""
            return (loc.first.inner_text(timeout=3000) or "")[:limit]
        except Exception:
            return ""

    # ---------- 解析 ----------
    # 岗位卡片的容器标签（实测 2026-11 版 zhipin 列表页）。
    # 曾经错误地只找 li.job-card-wrap，结果真实结构是：
    #   ul.rec-job-list > div.card-area > div.job-card-wrap > li.job-card-box
    # .job-card-wrap 是个 **div**，不是 li，所以旧选择器永远 count=0，
    # 页面明明有 45 张卡也只能给出「没有任何岗位卡片」。
    _CARD_SELECTORS = (
        "li.job-card-box",
        "li.job-card-wrap",
        "li.job-card-wrapper",
        ".job-card-box",
        ".job-card-wrapper",
    )

    def _card_container(self, page: Any) -> str:
        """返回当前页面里真正有卡片的那个选择器；都没有则返回空串。

        先按候选选择器逐个试，全部落空时退回「有 job_detail 链接的最近祖先
        列表项」这种结构性判定，Boss 下次换 class 名也不至于立刻瞎。
        """
        for sel in self._CARD_SELECTORS:
            try:
                if page.locator(sel).count() > 0:
                    return sel
            except Exception:
                continue
        return ""

    def _parse_dom_cards(self, page: Any) -> list[dict[str, Any]]:
        """接口捕获失败时的兜底：直接读页面上的岗位卡片。

        2026-11 版 zhipin 列表页的真实结构（实测）：
          ul.rec-job-list
            div.card-area
              div.job-card-wrap
                li.job-card-box            ← 卡片本身是 li.job-card-box
                  div.job-title
                    a.job-name            职位名 + /job_detail/ 链接
                    span.job-salary       薪资（innerText 里的数字常被字体加密，取不到就留空）
                  ul.tag-list li          经验 / 学历
                  a.boss-info
                    span.boss-name        公司名  ← 关键：不是 .company-name
                  span.company-location   城市·区·商圈
        只用 DOM 拿标题/公司/城市/经验/链接，JD 全文仍由 _fetch_detail 打开详情页补。
        """
        out: list[dict[str, Any]] = []
        sel = self._card_container(page)
        if not sel:
            return out
        try:
            cards = page.locator(sel)
            n = min(cards.count(), 60)
        except Exception:
            return out
        for i in range(n):
            try:
                c = cards.nth(i)
                link = ""
                if c.locator("a[href*='/job_detail/']").count():
                    link = c.locator("a[href*='/job_detail/']").first.get_attribute("href") or ""
                if not link:
                    continue
                if link.startswith("/"):
                    link = "https://www.zhipin.com" + link
                jid = link.split("?")[0].rstrip("/").split("/")[-1].replace(".html", "")
                if not jid:
                    continue

                title = ""
                if c.locator("a.job-name, .job-name").count():
                    title = (c.locator("a.job-name, .job-name").first.inner_text() or "").strip()

                salary = ""
                if c.locator("span.job-salary, .job-salary").count():
                    salary = (c.locator("span.job-salary, .job-salary").first.inner_text() or "").strip()

                company = ""
                for sel in ("span.boss-name", ".boss-name", ".company-name"):
                    if c.locator(sel).count():
                        company = (c.locator(sel).first.inner_text() or "").strip()
                        if company:
                            break

                location = ""
                if c.locator("span.company-location, .company-location").count():
                    location = (c.locator("span.company-location, .company-location").first.inner_text() or "").strip()

                exp = ""
                deg = ""
                # 卡片是 li，所以「li」在这个作用域里会匹配到卡片自己（inner_text
                # 是整张卡的文本，长度必然 >12 被跳过）。限定在 .tag-list 里找。
                tag_sel = "ul.tag-list li, .tag-list li, .job-tags span, .job-tags li"
                if c.locator(tag_sel).count():
                    lis = c.locator(tag_sel)
                    for k in range(min(lis.count(), 6)):
                        t = (lis.nth(k).inner_text() or "").strip()
                        if not t or len(t) > 12:
                            continue
                        if not exp and re.search(r"年|应届|在校|不限", t):
                            exp = t
                        elif not deg and re.search(r"本科|大专|硕士|博士|中专|高中", t):
                            deg = t

                out.append({
                    "job_id": f"boss:{jid}",
                    "job_url": link,
                    "title": title,
                    "salary": salary,
                    "company": company,
                    "location": location,
                    "experience": exp,
                    "degree": deg,
                    "platform": "boss",
                })
            except Exception:
                continue
        return out

    def _parse_search(self, raw: str) -> list[dict[str, Any]]:
        try:
            data = json.loads(raw)
        except json.JSONDecodeError:
            return []
        zp = data.get("zpData") or {}
        items = zp.get("jobList") or zp.get("jobCardList") or []
        out: list[dict[str, Any]] = []
        for it in items:
            if not isinstance(it, dict):
                continue
            jid = str(it.get("encryptJobId") or it.get("jobId") or "")
            if not jid:
                continue
            brand = it.get("brandName") or (it.get("brandInfo") or {}).get("brandName")
            out.append(
                {
                    "job_id": f"boss:{jid}",
                    "platform": "boss",
                    "title": it.get("jobName"),
                    "company": brand,
                    "salary": it.get("salaryDesc"),
                    "location": it.get("cityName") or it.get("areaDistrict"),
                    "experience": it.get("jobExperience"),
                    "degree": it.get("jobDegree"),
                    "company_industry": it.get("brandIndustry") or it.get("industry"),
                    "company_scale": it.get("brandScaleName") or it.get("brandScale"),
                    "company_stage": it.get("brandStageName") or it.get("brandStage"),
                    "hr_online": 1 if it.get("bossOnline") else 0,
                    "company_scale": it.get("brandScaleName"),
                    "company_stage": it.get("brandStageName"),
                    "hr_name": it.get("bossName"),
                    "hr_position": it.get("bossTitle"),
                    # HR 活跃度：Boss 列表接口里叫 bossActiveTimeDesc
                    # （如「今日活跃」「3日内活跃」「本周活跃」「2周内活跃」「1月内活跃」）
                    "hr_active_status": (
                        it.get("bossActiveTimeDesc")
                        or it.get("bossActiveStatus")
                        or (it.get("bossInfo") or {}).get("activeTimeDesc")
                    ),
                    "job_url": f"https://www.zhipin.com/job_detail/{jid}.html",
                    "jd_text": None,
                }
            )
        return out

    def _fetch_detail(self, page: Any, jid: str, item: dict[str, Any]) -> None:
        """进入岗位详情页，补齐 JD 全文 / HR 活跃度 / 公司规模·融资·行业 / 成立日期·注册资金。

        说明：Boss 详情页没有 job/detail.json 接口（实测只返回 404 或前端工具接口），
        所有信息都渲染在 DOM 里，因此按稳定的 class 结构解析。
        """
        # 请求详情页前随机等待 1500~9800ms（反爬）；等待可被「停止」立即打断
        t0 = time.time()
        wait_s = self.jitter()
        if self.should_stop():
            self.info("已停止：放弃打开这条详情")
            return
        self.info(f"等待 {wait_s:.1f}s 后打开岗位详情（{item.get('title') or jid}）")
        nav_ok = True
        try:
            page.goto(item["job_url"], wait_until="domcontentloaded", timeout=25000)
            self.stop.sleep(1.5)   # 可中断等待
        except Exception as exc:
            nav_ok = False
            self.warn(f"打开详情页失败：{exc}")
        try:
            self._parse_detail_dom(page, item)
        except Exception:
            pass
        if nav_ok and not (item.get("jd_text") or "").strip():
            try:
                cur = page.url
            except Exception:
                cur = "?"
            self.warn(
                f"详情页没解析到 JD（可能被风控重定向或页面未渲染完成）：{item.get('title') or jid} → {cur}"
            )
        jd_len = len(item.get("jd_text") or item.get("jd") or item.get("job_description") or "")
        self.info(
            f"详情{'已解析' if nav_ok else '未加载'}：JD {jd_len} 字，"
            f"耗时 {time.time() - t0:.1f}s"
        )
        # 暂停点：解析完不要立刻翻页，先把页面停在当前 JD 上，
        # 让用户能切到浏览器里观察这条 JD 的详情页
        if self.stop.is_paused():
            self.info(f"已暂停，页面停留在：{item.get('title') or jid}")
            self.stop.wait_if_paused()
            self.info("已恢复，继续抓取")

    def _parse_detail_dom(self, page: Any, item: dict[str, Any]) -> None:
        # ---------- 基本信息 ----------
        h1 = _first_text(page, ".job-banner .info-primary h1, .info-primary h1, h1")
        if h1:
            item["title"] = h1
        salary = _first_text(page, ".job-banner .salary, .info-primary .salary")
        if salary:
            item["salary"] = salary
        # 「上海 经验不限 本科」三段
        meta = _first_text(page, ".job-banner .info-primary p, .info-primary p")
        if meta:
            parts = re.split(r"\s{2,}|\s", meta.strip())
            parts = [p for p in parts if p]
            if parts:
                item["location"] = parts[0]
            for p in parts[1:]:
                if re.search(r"年|应届|在校|不限", p) and not item.get("experience"):
                    item["experience"] = p
                elif re.search(r"本科|大专|硕士|博士|中专|高中|学历", p) and not item.get("degree"):
                    item["degree"] = p

        # ---------- JD 全文 ----------
        jd = ""
        loc = page.locator(".job-detail-section .job-sec-text, .job-detail .job-sec-text")
        for i in range(loc.count()):
            try:
                t = (loc.nth(i).inner_text() or "").strip()
            except Exception:
                continue
            if t:
                jd += ("\n\n" if jd else "") + t
        if not jd:
            # 兜底：详情区块整块取（用 text_content 而不是 innerText，
            # 避免元素在视口外尚未渲染时 innerText 返回空串）
            for sel in (".job-detail-section", ".job-detail", "#main .job-detail-section"):
                try:
                    if page.locator(sel).count():
                        raw = page.locator(sel).first.text_content() or ""
                        if raw.strip():
                            jd = raw.strip()
                            break
                except Exception:
                    continue
        if not jd:
            try:
                body = page.evaluate("() => document.querySelector('#main')?.innerText || ''") or ""
                if body.strip():
                    jd = body.strip()
            except Exception:
                pass
        if jd:
            item["jd_text"] = jd[:8000]

        # ---------- HR 活跃度 / HR 姓名 ----------
        banner = _first_text(page, ".job-banner, .job-primary, .info-company")
        m = re.search(r"(在线|刚刚|今日活跃|今日|今天活跃|本周活跃|3日内活跃|三日活跃|"
                      r"本月活跃|2周内活跃|1周内活跃|1月内活跃|\d+日内活跃|\d+小时内活跃|"
                      r"\d+天前活跃|\d+月前活跃|半年内活跃)", banner or "")
        if m:
            item["hr_active_status"] = m.group(1)
        hr_name = _first_text(page, ".job-banner .info-company a.name, .job-banner .boss-info .name, .media .name")
        if hr_name:
            item["hr_name"] = hr_name
        hr_title = _first_text(page, ".job-banner .boss-info-wrap .boss-info-attr, .boss-info .gray")
        if hr_title:
            item["hr_position"] = hr_title

        # ---------- 公司名 ----------
        # 之前完全没解析公司名：搜索接口的 brandName 只在有 company 对象时才有，
        # DOM 兜底抓到的卡片里也没有，导致大量记录公司名为空。
        # 这里按「侧栏公司卡 → 公司信息区 → 招聘者信息区」的顺序兜底。
        if not (item.get("company") or "").strip():
            # 详情页最可靠：右侧公司卡 .sider-company 的第 2 行（第 1 行是"公司基本信息"）
            try:
                if page.locator(".sider-company").count():
                    lines = [
                        ln.strip()
                        for ln in (page.locator(".sider-company").first.inner_text() or "").split("\n")
                        if ln.strip()
                    ]
                    for ln in lines:
                        if ln in ("公司基本信息", "公司介绍", "查看全部职位"):
                            continue
                        if re.search(r"人|轮|上市|融资", ln):
                            continue
                        if 2 <= len(ln) <= 40:
                            item["company"] = ln
                            break
            except Exception:
                pass
        if not (item.get("company") or "").strip():
            for sel, limit in (
                (".sider-company .company-name", 40),
                (".sider-company h3", 40),
                (".company-info-box .company-name", 40),
                (".job-banner .info-company h3.company-name", 40),
                (".job-banner .info-company .company-name", 40),
                (".job-banner .company-info a", 40),
            ):
                txt = _first_text(page, sel, limit)
                if txt and re.search(r"[\u4e00-\u9fa5A-Za-z]", txt):
                    item["company"] = txt.strip()
                    break

        # ---------- 右侧公司卡：融资 / 规模 / 行业 ----------
        sider = _first_text(page, ".sider-company")
        if sider:
            lines = [ln.strip() for ln in sider.split("\n") if ln.strip()]
            skip = {"查看全部职位", "公司基本信息"}
            for ln in lines:
                if re.search(r"\d+\s*-\s*\d+\s*人|\d+人以上|少于\d+人", ln):
                    item.setdefault("company_scale", ln)
                elif ln in ("未融资", "天使轮", "A轮", "B轮", "C轮", "D轮及以上", "已上市", "不需要融资") or "轮" in ln:
                    item.setdefault("company_stage", ln)
            # 行业：公司卡里 icon-industry 那个链接最准
            ind = _first_text(page, ".sider-company .icon-industry")
            if not ind:
                ind = _first_text(page, ".sider-company a[ka='job-detail-brandindustry']")
            if ind:
                item["company_industry"] = ind.strip()[:20]
            else:
                # 兜底：跳过公司名/融资/规模行，取第一段短的纯中文
                brand = _first_text(page, ".sider-company .company-info")
                for ln in lines:
                    if ln in skip or ln == brand:
                        continue
                    if re.search(r"人|轮|上市|融资", ln):
                        continue
                    if re.fullmatch(r"[\u4e00-\u9fa5A-Za-z/·]{2,12}", ln):
                        item["company_industry"] = ln
                        break

        # ---------- 工商信息：成立日期 / 注册资金 / 公司全称 ----------
        for cls, key in (("li.res-time", "founded_date"), ("li.company-fund", "reg_capital"),
                         ("li.company-name", "company_full"), ("li.company-type", "company_type")):
            text = _first_text(page, f".business-info-box {cls}, {cls}")
            if not text:
                continue
            val = re.sub(r"^(成立日期|注册资金|公司名称|企业类型)\s*", "", text.replace("\n", " ")).strip()
            if not val:
                continue
            if key == "founded_date":
                mm = re.search(r"(\d{4})-?(\d{1,2})?-?(\d{1,2})?", val)
                if mm:
                    y = int(mm.group(1))
                    if 1900 < y <= datetime.now().year:
                        item["founded_year"] = y
                        item["founded_years"] = max(0, datetime.now().year - y)
            else:
                item[key] = val[:120]

        # ---------- 公司介绍 ----------
        intro = _first_text(page, ".company-info-box .job-sec-text, .job-detail-company .job-sec-text")
        if intro:
            item["company_intro"] = re.sub(r"\s*查看全部\s*$", "", intro)[:2000]
        addr = _first_text(page, ".job-location .location-address, .company-address .job-location")
        if addr:
            item["company_address"] = addr.strip()[:160]

    def _parse_company_text(self, text: str, item: dict[str, Any]) -> None:
        """从「互联网 · 500-999人 · 已上市」这类文本里拆出公司规模/融资/行业。"""
        parts = [p.strip() for p in text.replace("，", "·").split("·") if p.strip()]
        for part in parts:
            if re.search(r"\d+\s*-\s*\d+\s*人|\d+人以上|少于\d+人", part):
                item.setdefault("company_scale", part)
            elif part in ("未融资", "天使轮", "A轮", "B轮", "C轮", "D轮及以上", "已上市", "不需要融资") or "轮" in part:
                item.setdefault("company_stage", part)
            elif not item.get("company_industry") and len(part) <= 12:
                item.setdefault("company_industry", part)

# ---------------- Boss 官网筛选参数 ----------------
# Boss 岗位列表页的真实筛选项全是**重复键**多选，例如：
#   .../web/geek/job?query=Java&city=101020100&salary=406&salary=407&experience=105
# 同一维度内多项之间是 OR，不同维度之间是 AND。

def _multi(params: list[str], name: str, values: list[str]) -> None:
    for v in values or []:
        if v:
            params.append(f"{name}={quote(str(v))}")


# 自定义/预置薪资档位 -> Boss 官方 salary 编码（找不到对应编码时返回 None，
# 由后端在入库前按区间二次校验兜底）
# Boss 官方「薪资待遇」下拉只有这 6 档，URL 里的 salary 编码也只认这几个
# Boss 官方「薪资待遇」下拉只有 6 档：不限 / 3K以下 / 3-5K / 5-10K / 10-20K / 20-50K / 50K以上
# URL 里的 salary 编码也只认这几个（403=3K以下 404=3-5K 405=5-10K 414=10-20K 415=20-50K 418=50K以上）
_BOSS_TIER_CODES: list[tuple[int, int, str]] = [
    (0, 3, "403"), (3, 5, "404"), (5, 10, "405"), (10, 20, "414"),
    (20, 50, "415"), (50, 9999, "418"),
]


def map_salary_code(code: str) -> str | None:
    """把界面上的薪资编码映射成 Boss 的 salary 编码（映射不到就给 None，由后端二次校验兜底）。"""
    code = str(code or "").strip()
    if not code:
        return None
    if code.startswith("sal:"):
        try:
            low, high = (int(x) for x in code[4:].split("-"))
        except Exception:
            return None
        best, cover_best = None, 0
        for tlo, thi, tcode in _BOSS_TIER_CODES:
            cover = min(high, thi) - max(low, tlo)
            if cover > 0 and cover > cover_best:
                best, cover_best = tcode, cover
        if best:
            return best
        for tlo, thi, tcode in _BOSS_TIER_CODES:
            if tlo <= low and high <= thi:
                return tcode
        return None
    # 老版本数字编码原样透传
    return code


def build_search_url(cfg: Any, keyword: str) -> str:
    """按 Boss 官网的真实参数拼岗位列表 URL。"""
    params = [f"query={quote(keyword)}", f"city={quote(cfg.city or '101020100')}"]
    if cfg.job_type:
        params.append(f"jobType={quote(cfg.job_type)}")
    # 薪资：Boss 只认它自己的数字编码；自定义档位（sal:18-25）没有对应编码，
    # 这时后端在入库前用 salary_out_of_range 二次校验，所以这里能映射就映射，不能就跳过。
    _multi(params, "salary", [map_salary_code(c) for c in (cfg.salary or []) if map_salary_code(c)])
    _multi(params, "experience", cfg.experience)
    _multi(params, "degree", cfg.degree)
    _multi(params, "scale", cfg.scale)
    _multi(params, "stage", cfg.stage)
    _multi(params, "industry", cfg.industry)
    return SEARCH_BASE + "?" + "&".join(params)


_LABEL = {
    "salary": "薪资", "experience": "经验", "degree": "学历",
    "scale": "规模", "stage": "融资", "industry": "行业",
}


def describe_filter(cfg: Any) -> str:
    parts = []
    for name, label in _LABEL.items():
        vals = getattr(cfg, name, None) or []
        if vals:
            parts.append(f"{label}×{len(vals)}")
    return "、".join(parts) if parts else "不限"


_TAG_RE = re.compile(r"<[^>]+>")


def _strip_html(html: str) -> str:
    text = _TAG_RE.sub("\n", html or "")
    text = text.replace("&nbsp;", " ").replace("&amp;", "&").replace("&lt;", "<").replace("&gt;", ">")
    lines = [ln.strip() for ln in text.split("\n")]
    return "\n".join(ln for ln in lines if ln)


def _first_text(page: Any, selector: str, limit: int = 4000) -> str:
    """安全取第一个匹配元素的内文本；找不到/出错都返回空串。"""
    try:
        loc = page.locator(selector)
        if loc.count() == 0:
            return ""
        return (loc.first.inner_text() or "").strip()[:limit]
    except Exception:
        return ""
