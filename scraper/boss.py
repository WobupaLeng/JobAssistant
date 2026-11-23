"""Boss 直聘抓取：搜索页 DOM 卡片 + 详情页 DOM 解析。

从 `backend/scrapers/boss.py`（855 行）平移。**反爬对抗逻辑一行都没有重写**，
只做三件事：
1. 剥掉打分类/入库类耦合（`self.arrived`、`on_job` 回调、`target_count` 早停），
   这些现在归 Rust 侧——sidecar 只负责「抓到什么就吐什么」；
2. 日志改走回调（由 sidecar 转成 `log` 事件）；
3. 停止信号由 `StopFlag` 换成一个 `should_stop()` 回调（sidecar 用 stdin 读到
   stop 时置位），避免线程与 stdin 读循环互相干扰。

**数据来源只有 DOM，不再监听接口响应**（2026-11-21 用户决策「默认不在使用接口捕获数据」）。
原来那条 `page.on("response")` 抓 `joblist.json` 的通道已整体删除，理由不止是「不要用」：
- 接口通道**丢字段**：`_parse_dom_cards` 已实测能拿到城市·区·商圈（`span.company-location`），
  而接口只给一个 `cityName`（如「上海」），没有区；公司名、薪资口径两边也不一致。
- **不翻页就没数据**：接口通道只在 `page.goto` 时随首屏请求触发，下面的翻页是点
  「下一页」按钮，列表异步刷新、不重新导航，所以第 2 页起接口通道**恒为空**，
  全靠 DOM 兜底——也就是说接口通道实际上只在第 1 页起作用，代价却是两套字段口径并存。
- 接口名与字段随时会被改（`joblist.json` / `zpData.jobList` / `bossActiveTimeDesc`），
  DOM 结构反而更稳定，且岗位链接里的 `encryptJobId` 与详情页一一对应。

DOM 通道拿不到、而接口拿得到的只有拿 `hr_active_status`（卡片上没有），
该字段目前留空；规则 `exclude_hr_active_days` 因此对抓来的岗位不生效（见 DEVELOPMENT.md）。
"""
from __future__ import annotations

import re
import time
from datetime import datetime
from typing import Any, Callable
from urllib.parse import quote

# 城市编码：Boss 只认自己的数字编码
CITY_CODE = {"上海": "101020100"}
# 2026-11-21 实测：站点已从 /web/geek/job 迁到 /web/geek/jobs（多了 s），
# 旧地址会被 302 过去。直接写新地址，省一次跳转与一次「加载中」页风险。
SEARCH_BASE = "https://www.zhipin.com/web/geek/jobs"

# ---- Boss 官网筛选参数 ----
# 真实筛选项全是**重复键**多选，例如：
#   .../web/geek/job?query=Java&city=101020100&salary=406&salary=407&experience=105
# 同一维度内多项之间是 OR，不同维度之间是 AND。

# 薪资：Boss 官方下拉只有 6 档，URL 编码也只认这几个
#   403=3K以下 404=3-5K 405=5-10K 414=10-20K 415=20-50K 418=50K以上
_BOSS_TIER_CODES: list[tuple[int, int, str]] = [
    (0, 3, "403"), (3, 5, "404"), (5, 10, "405"), (10, 20, "414"),
    (20, 50, "415"), (50, 9999, "418"),
]

_TAG_RE = re.compile(r"<[^>]+>")


def decode_boss_salary(text: str | None) -> str:
    """还原 Boss 私用区字符的薪资。

    Boss 把薪资数字渲染成 U+E031..U+E03A 的私用区字符（字体加密反爬），
    直接读 innerText 会得到一堆不可见字符。这里把它们映射回 0-9。
    其它私用区字符（如「薪」图标）直接丢弃。
    """
    if not text:
        return ""
    out: list[str] = []
    for ch in str(text):
        cp = ord(ch)
        if 0xE031 <= cp <= 0xE03A:
            out.append(str(cp - 0xE031))
        elif 0xE000 <= cp <= 0xF8FF:
            continue
        else:
            out.append(ch)
    return "".join(out)


def _multi(params: list[str], name: str, values: list[str] | None) -> None:
    for v in values or []:
        if v:
            params.append(f"{name}={quote(str(v))}")


def map_salary_code(code: str) -> str | None:
    """把界面薪资编码映射成 Boss 的 salary 编码（映射不到返回 None）。"""
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
    return code


def build_search_url(cfg: Any, keyword: str) -> str:
    """按 Boss 官网的真实参数拼岗位列表 URL。"""
    params = [f"query={quote(keyword)}", f"city={quote(cfg.city or '101020100')}"]
    if cfg.job_type:
        params.append(f"jobType={quote(cfg.job_type)}")
    # 自定义档位（sal:18-25）Boss 没有对应编码，后端会用区间二次校验兜底
    _multi(params, "salary", [map_salary_code(c) for c in (cfg.salary or []) if map_salary_code(c)])
    _multi(params, "experience", cfg.experience)
    _multi(params, "degree", cfg.degree)
    _multi(params, "scale", cfg.scale)
    _multi(params, "stage", cfg.stage)
    _multi(params, "industry", cfg.industry)
    return SEARCH_BASE + "?" + "&".join(params)


def _first_text(page: Any, selector: str, limit: int = 4000) -> str:
    """安全取第一个匹配元素的内文本；找不到/出错都返回空串。"""
    try:
        loc = page.locator(selector)
        if loc.count() == 0:
            return ""
        return (loc.first.inner_text() or "").strip()[:limit]
    except Exception:
        return ""


def _last_line(text: str) -> str:
    """取非空最后一行。用于 `p.boss-info-attr` 这种 "公司\\n·\\n职位" 的结构。"""
    lines = [ln.strip() for ln in (text or "").splitlines() if ln.strip()]
    return lines[-1] if lines else ""


def _strip_html(html: str) -> str:
    text = _TAG_RE.sub("\n", html or "")
    text = (text.replace("&nbsp;", " ").replace("&amp;", "&")
                .replace("&lt;", "<").replace("&gt;", ">"))
    lines = [ln.strip() for ln in text.split("\n")]
    return "\n".join(ln for ln in lines if ln)


class BossScraper:
    """一次抓取任务。用一次 new 一个，不复用（状态全在实例上）。"""

    platform = "boss"

    # 岗位卡片容器（实测 2026-11 版 zhipin 列表页）。
    # 曾经只找 li.job-card-wrap 导致 count 恒为 0：真实结构是
    #   ul.rec-job-list > div.card-area > div.job-card-wrap > li.job-card-box
    # .job-card-wrap 是 **div** 不是 li，所以旧选择器永远匹配不到卡片。
    _CARD_SELECTORS = (
        "li.job-card-box",
        "li.job-card-wrap",
        "li.job-card-wrapper",
        ".job-card-box",
        ".job-card-wrapper",
    )

    def __init__(
        self,
        cfg: Any,
        log: Callable[[str, str], None],
        should_stop: Callable[[], bool],
        sleep: Callable[[float], bool],
        on_job: Callable[[dict[str, Any]], None],
    ) -> None:
        self.cfg = cfg
        self.log = log
        self._should_stop = should_stop
        self._sleep = sleep          # 可中断睡眠：返回 True 表示期间被要求停止
        self.on_job = on_job
        self.collected = 0
        # 非空 = 本次抓取因风控被停（sidecar 据此发 scrape.risk 事件）
        self.risk_reason = ""

    # ---------- 工具 ----------
    def info(self, msg: str) -> None:
        self.log("info", f"[{self.platform}] {msg}")

    def warn(self, msg: str) -> None:
        self.log("warn", f"[{self.platform}] {msg}")

    def should_stop(self) -> bool:
        return self._should_stop()

    def jitter(self, lo: float | None = None, hi: float | None = None) -> float:
        """随机延时（反爬）：默认 1500~9800ms；睡眠可被停止立即打断。"""
        import random

        a = self.cfg.delay_min if lo is None else lo
        b = self.cfg.delay_max if hi is None else hi
        if b < a:
            a, b = b, a
        secs = random.uniform(a, b)
        self._sleep(secs)
        if self.should_stop():
            return 0.0
        return secs

    # ---------- 主流程 ----------
    def run(self, page: Any) -> int:
        cfg = self.cfg
        jobs: dict[str, dict[str, Any]] = {}
        hit_kw: dict[str, str] = {}
        # 多关键词之间串行，Boss 的 query 只支持一个词。
        # 卡片总数上限由 max_cards（默认 100）与 daily_limit 共同约束，见 _search_keyword。
        keywords = list(cfg.keywords or [""])

        for kw in keywords:
            if self.should_stop():
                break
            self.info(f"关键词「{kw}」开始搜索")
            self._search_keyword(page, kw, jobs, hit_kw)

        # 补齐命中关键词后统一交给 Rust 侧
        for jid, kw in hit_kw.items():
            if jid in jobs:
                jobs[jid]["matched_keyword"] = kw
        self.collected = len(jobs)
        self.info(f"采集完成，共 {self.collected} 条")
        return self.collected

    def _search_keyword(
        self,
        page: Any,
        kw: str,
        jobs: dict[str, dict[str, Any]],
        hit_kw: dict[str, str],
    ) -> None:
        """单个关键词的完整采集：**收卡片 → 滚动 → 再收卡片 → 拉详情**，交替进行。

        2026-11-21 改版说明。旧实现是「一次性收完当页卡片 → 把当页全部详情拉完 →
        才滚动」：第 1 页 15 条详情每条 3~6 秒，加起来 55~90 秒才轮到滚动，
        用户要等一分半才能看到第 16 条，且中途停止就只剩 15 条候选。

        现在改成滚动与详情交替：每收一批新卡片就立刻向下滚动，把下一批卡片
        先抓到手，再回头拉这一批的详情。好处有两个：
        1. 候选池边抓边涨，不会因为提前停止而只剩一页；
        2. 卡片解析不依赖网络往返，比详情页快一个数量级，滚动是「顺手」完成的。

        注意站点没有「下一页」按钮（旧选择器 `a.ui-icon-arrow-right` 永远匹配
        不到），只能靠无限滚动。
        """
        cfg = self.cfg
        processed: set[str] = set()
        # 卡片总数上限：max_cards（默认 100 条）与 daily_limit 取小，且不小于 1。
        # 用户口径「最大滚动 100 条」= 一次运行最多加载 100 张卡片，滚到这里就停。
        card_cap = min(
            int(getattr(cfg, "max_cards", 0) or 100),
            int(getattr(cfg, "daily_limit", 0) or 100),
        )
        if card_cap <= 0:
            card_cap = 100

        # 预热：先回首页再带筛选条件进搜索页。
        # 直接带 salary/experience 多参数跳转会触发「加载中/风控」页，
        # 页面根本不会渲染出岗位卡片（实测）。
        try:
            page.goto("https://www.zhipin.com/web/geek/job",
                      wait_until="domcontentloaded", timeout=30000)
            self._sleep(1.2)
        except Exception:
            pass

        url = build_search_url(cfg, kw)
        self.info(f"关键词「{kw}」开始进第 1 页")
        try:
            page.goto(url, wait_until="domcontentloaded", timeout=30000)
            self._sleep(3.5)
        except Exception as exc:
            self.warn(f"打开搜索页失败：{exc}")
            return

        def _harvest_dom() -> int:
            added = 0
            for item in self._parse_dom_cards(page):
                jid = item.get("job_id")
                if not jid or jid in jobs:
                    continue
                jobs[jid] = item
                hit_kw.setdefault(jid, kw)
                added += 1
            return added

        added_dom = _harvest_dom()

        # 空页处理：先诊断再重试，重试还空就放弃这个关键词
        empty_retry = 0
        while not added_dom and empty_retry < 2 and not self.should_stop():
            empty_retry += 1
            reason = self._diagnose_empty_page(page)
            self.warn(f"首屏未拿到岗位：{reason}（第 {empty_retry} 次重试）")
            try:
                # 退一步：先回首页去掉全部筛选参数，再带着条件重进
                page.goto("https://www.zhipin.com/web/geek/job",
                          wait_until="domcontentloaded", timeout=30000)
                self._sleep(2.0)
                page.goto(url, wait_until="domcontentloaded", timeout=30000)
                self._sleep(3.5)
            except Exception as exc:
                self.warn(f"重试打开搜索页失败：{exc}")
            added_dom = _harvest_dom()

        if not added_dom:
            # 检测即停：命中安全验证页就不再重试/继续，记下原因交给 sidecar 发 scrape.risk
            risk = self._detect_risk(page)
            if risk:
                self.risk_reason = risk
                self.warn(f"触发风控，已停止抓取：{risk}")
                self.warn("请在浏览器中手动完成验证后，再重新开始抓取")
                return
            self.warn(
                "首屏没有拿到任何岗位（页面卡片为空），"
                f"可能触发风控或未登录。当前地址：{page.url}"
            )
            self.warn("建议：先完成扫码登录，再重新开始抓取")
            return

        # 只处理这个关键词范围内的卡片（jobs 是跨关键词共享的，
        # 否则第二个关键词会把第一个关键词已抓过的卡片再拉一遍详情）
        mine: list[str] = [jid for jid, kw_of in hit_kw.items() if kw_of == kw]

        batch_no = 0
        while not self.should_stop():
            batch_no += 1
            self.info(f"第 {batch_no} 批：新增 {added_dom} 条卡片（累计 {len(mine)}）")

            # 先把已收卡片的详情拉完（流式回传，拿到一条就发给 Rust）
            fresh = [(jid, jobs[jid]) for jid in mine if jid not in processed]
            for idx, (jid, item) in enumerate(fresh, 1):
                if self.should_stop():
                    break
                processed.add(jid)
                # 详情拿不到就不推送：没有 JD 的条目入库后会被规则判
                # 「无 JD 不入库」，白占一条数据库记录，还会让用户以为抓到了
                ok = self._fetch_detail(page, jid, item)
                item["matched_keyword"] = hit_kw.get(jid)
                if ok:
                    try:
                        self.on_job(item)
                    except Exception as exc:
                        self.warn(f"回传岗位失败：{exc}")
                else:
                    self.info(f"跳过（详情未取到）：{item.get('title') or jid}")
                if idx % 10 == 0:
                    self.info(f"详情进度 {idx}/{len(fresh)}")
                self._sleep(0.3)

            if self.should_stop():
                break
            if len(mine) >= card_cap:
                self.info(f"已达卡片上限 {card_cap} 条，停止加载（当前 {len(mine)} 条）")
                break

            # 详情拉完再滚动取下一批：此时页面还停在刚才的位置，滚动才有效
            if not self._load_more_by_scroll(page, len(mine)):
                self.info(f"已无更多岗位，关键词「{kw}」结束（共 {len(mine)} 条）")
                break
            added_dom = _harvest_dom()
            if not added_dom:
                self.info(f"滚动后没有新卡片，关键词「{kw}」结束")
                break
            mine = [jid for jid, kw_of in hit_kw.items() if kw_of == kw]
            if len(mine) > card_cap:
                self.info(f"批次超限（{len(mine)} > {card_cap}），本次最多取 {card_cap} 条")
            self.jitter()

    def _load_more_by_scroll(self, page: Any, before: int, rounds: int = 4) -> bool:
        """向下滚动触发无限加载，返回是否真的拿到了新卡片。

        判定依据是**卡片数量增长**而不是「滚动成功」：滚到底后页面可能只是回弹，
        不判断数量就会无限翻下去。

        `before` 是调用方眼中的「已收卡片数」；递增到不能再涨就判定到底。
        注意每次滚动后要等网络把新卡片追加进 DOM，2.5 秒是实测的稳妥值。
        """
        prev = before
        for _ in range(rounds):
            try:
                page.mouse.wheel(0, 4000)
            except Exception:
                return False
            self._sleep(2.5)
            try:
                now = page.locator("li.job-card-box").count()
            except Exception:
                return False
            if now > prev:
                return True
            prev = now
        return False

    def _detect_risk(self, page: Any) -> str:
        """只识别「安全验证」类风控页，返回原因；不是风控返回空串。

        与 _diagnose_empty_page 的区别：这里只关心必须人工介入的验证页，
        登录失效 / 加载中 / 无结果都不算风控，不能因此误停。
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
            text = _first_text(page, "body", 3000)
            for marker in ("安全验证", "请完成验证", "拖动滑块", "点击验证", "人机验证"):
                if marker in text:
                    return f"页面要求安全验证（{marker}）"
        except Exception:
            pass
        return ""

    def _diagnose_empty_page(self, page: Any) -> str:
        """空页到底是哪种：验证码/风控、登录失效、加载中、还是真没有结果。

        这段判断直接决定用户下一步该干什么，所以把页面特征翻成人话。
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
            text = _first_text(page, "body", 3000)
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
            links = 0
            for sel in ("a[href*='/job_detail/']", "a[href*='job_detail']"):
                try:
                    links = max(links, page.locator(sel).count())
                except Exception:
                    pass
            if links and self._card_container(page):
                return "卡片找到了但没解析出有效字段，正在重试"
            if links:
                return (f"页面里有 {links} 个岗位链接，但卡片选择器一个都没匹配上"
                        f"（标题「{title}」，页面结构可能变了）")
            snippet = " ".join(text.split())[:60]
            return f"页面标题「{title}」没有任何岗位卡片（正文开头：{snippet}）"
        except Exception as exc:
            return f"无法判断空页原因（{exc}）"

    def _card_container(self, page: Any) -> str:
        """返回当前页面里真正有卡片的那个选择器；都没有则返回空串。"""
        for sel in self._CARD_SELECTORS:
            try:
                if page.locator(sel).count() > 0:
                    return sel
            except Exception:
                continue
        return ""

    # ---------- 解析 ----------
    def _parse_dom_cards(self, page: Any) -> list[dict[str, Any]]:
        """接口捕获失败时的兜底：直接读页面上的岗位卡片。

        真实结构（实测 2026-11）：
          ul.rec-job-list > div.card-area > div.job-card-wrap > li.job-card-box
            div.job-title > a.job-name（职位名 + /job_detail/ 链接）
            span.job-salary（数字常被字体加密，取不到就留空）
            ul.tag-list li（经验 / 学历）
            a.boss-info > span.boss-name（**公司名在这里，不是 .company-name**）
            span.company-location（城市·区·商圈）
        只拿列表页字段，JD 全文由 _fetch_detail 打开详情页补。
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
                for s in ("span.boss-name", ".boss-name", ".company-name"):
                    if c.locator(s).count():
                        company = (c.locator(s).first.inner_text() or "").strip()
                        if company:
                            break

                location = ""
                if c.locator("span.company-location, .company-location").count():
                    location = (c.locator("span.company-location, .company-location").first.inner_text() or "").strip()

                exp = ""
                deg = ""
                # 卡片本身是 li，所以「li」会匹配到卡片自己（inner_text 是整张卡的
                # 文本，长度必然 >12 被跳过）。必须限定在 .tag-list 里找。
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
                    "platform": "boss",
                    "job_url": link,
                    "title": title,
                    # 卡片里的薪资是私用区字符，这里还原成真数字
                    "salary": decode_boss_salary(salary),
                    "company": company,
                    "location": location,
                    "experience": exp,
                    "degree": deg,
                    "jd_text": None,
                })
            except Exception:
                continue
        return out

    def _fetch_detail(self, page: Any, jid: str, item: dict[str, Any]) -> bool:
        """打开详情页，补齐 JD 全文 / HR 活跃度 / 公司规模·融资·行业 / 成立年份。

        Boss 详情页没有可用的 detail.json 接口（实测只返回 404），
        所有信息都渲染在 DOM 里，因此按稳定的 class 结构解析。

        返回 True 表示**取到了 JD**（这是入库的硬前提）。返回 False 的情况：
        被停止信号打断、导航失败、页面被风控重定向、或解析不到 JD。
        """
        wait_s = self.jitter()
        if self.should_stop():
            self.info("已停止：放弃打开这条详情")
            return False
        self.info(f"等待 {wait_s:.1f}s 后打开详情（{item.get('title') or jid}）")
        nav_ok = True
        try:
            page.goto(item["job_url"], wait_until="domcontentloaded", timeout=25000)
            self._sleep(1.5)
        except Exception as exc:
            nav_ok = False
            self.warn(f"打开详情页失败：{exc}")
        try:
            self._parse_detail_dom(page, item)
        except Exception as exc:
            self.warn(f"解析详情页异常：{type(exc).__name__}: {exc}")
        if nav_ok and not (item.get("jd_text") or "").strip():
            try:
                cur = page.url
            except Exception:
                cur = "?"
            self.warn(
                f"详情页没解析到 JD（可能被风控重定向或未渲染完）："
                f"{item.get('title') or jid} → {cur}"
            )
        return bool((item.get("jd_text") or "").strip())

    def _parse_detail_dom(self, page: Any, item: dict[str, Any]) -> None:
        # ---------- 基本信息 ----------
        h1 = _first_text(page, ".job-banner .info-primary h1, .info-primary h1, h1")
        if h1 and len(h1) <= 120:
            item["title"] = h1
        # 详情页顶部那个 salary 才是本岗位的。注意 .salary 这个宽选择器会命中页面里
        # 所有推荐岗位的薪资（实测 37 个），只能作为最后兜底且必须带范围限制
        salary = _first_text(page, ".job-banner .info-primary .salary, .job-banner .salary, .info-primary .salary")
        if not salary:
            salary = _first_text(page, ".job-banner .salary")
        if salary:
            item["salary"] = decode_boss_salary(salary).strip()
        # 「上海 经验不限 本科」三段
        meta = _first_text(page, ".job-banner .info-primary p, .info-primary p")
        if meta:
            parts = [p for p in re.split(r"\s{2,}|\s", meta.strip()) if p]
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
            # 兜底：整块取。用 text_content 而非 innerText——元素在视口外
            # 尚未渲染时 innerText 会返回空串
            for sel in (".job-detail-section", ".job-detail", "#main .job-detail-section"):
                try:
                    if page.locator(sel).count():
                        raw = page.locator(sel).first.text_content() or ""
                        if raw.strip():
                            jd = _strip_html(raw).strip()
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

        # ---------- HR 活跃度 / 姓名 / 职位 ----------
        # 活跃度在 `.job-boss-info` 区块（实测：`.boss-active-time` → "刚刚活跃"），
        # 不是老版本的 `.job-banner`——那里的 `.boss-info-attr` 在本站已不存在，
        # 所以此前 `hr_active_status` 一直是空，规则 exclude_hr_active_days 静默失效。
        active = _first_text(page, ".job-boss-info .boss-active-time, .boss-active-time")
        if not active:
            # 兜底：整块老板信息里正则捞一次，避免文案换了标记就取不到
            active = _first_text(page, ".job-boss-info")
        if active:
            m = re.search(
                r"(在线|刚刚|[今日本周本月今天]+活跃|[一二两三四五六七八九十\d]+日内活跃|"
                r"\d+\s*小时内活跃|\d+\s*天前活跃|\d+\s*月前活跃|半年内活跃|"
                r"本周回复|今日回复|刚刚回复)",
                active,
            )
            if m:
                item["hr_active_status"] = m.group(1)
        # `.name` 的 innerText 是 "姓名\n活跃度" 两行；`em.name` 才是纯姓名。
        hr_name = _first_text(page, ".job-boss-info em.name, .job-boss-info .name")
        if not hr_name:
            hr_name = _first_text(
                page, ".job-banner .info-company a.name, .boss-info .name, .media .name"
            )
        if hr_name:
            item["hr_name"] = hr_name.split("\n", 1)[0].strip() or hr_name
        # 职位在 `.boss-info-pub`（纯职位）或 `p.boss-info-attr`（"公司\n·\n职位"）。
        hr_title = _first_text(page, ".job-boss-info p.boss-info-pub, .job-boss-info .boss-info-pub")
        if not hr_title:
            raw = _first_text(page, ".job-boss-info p.boss-info-attr, .job-boss-info .boss-info-attr")
            if raw:
                hr_title = _last_line(raw)
        if not hr_title:
            hr_title = _first_text(
                page,
                ".job-banner .boss-info-wrap .boss-info-attr, .boss-info .gray",
            )
        if hr_title:
            item["hr_position"] = hr_title.strip().lstrip("·").strip() or hr_title

        # ---------- 公司名 ----------
        # 现行版曾完全没解析公司名：搜索接口的 brandName 只在有 company 对象时才有，
        # DOM 兜底的卡片里也没有，导致大量记录公司名为空。
        # 这里按「侧栏公司卡 → 公司信息区 → 招聘者信息区」的顺序兜底。
        if not (item.get("company") or "").strip():
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
            ind = _first_text(page, ".sider-company .icon-industry")
            if not ind:
                ind = _first_text(page, ".sider-company a[ka='job-detail-brandindustry']")
            if ind:
                item["company_industry"] = ind.strip()[:20]
            else:
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
        for cls, key in (
            ("li.res-time", "founded_date"),
            ("li.company-fund", "reg_capital"),
            ("li.company-name", "company_full"),
            ("li.company-type", "company_type"),
        ):
            text = _first_text(page, f".business-info-box {cls}, {cls}")
            if not text:
                continue
            val = re.sub(r"^(成立日期|注册资金|公司名称|企业类型)\s*", "",
                         text.replace("\n", " ")).strip()
            if not val:
                continue
            if key == "founded_date":
                mm = re.search(r"(\d{4})-?(\d{1,2})?-?(\d{1,2})?", val)
                if mm:
                    y = int(mm.group(1))
                    if 1900 < y <= datetime.now().year:
                        item["founded_years"] = max(0, datetime.now().year - y)
            elif key == "reg_capital":
                item["reg_capital"] = val[:120]

        # ---------- 公司介绍 / 地址 ----------
        intro = _first_text(page, ".company-info-box .job-sec-text, .job-detail-company .job-sec-text")
        if intro:
            item["company_intro"] = re.sub(r"\s*查看全部\s*$", "", intro)[:2000]
        addr = _first_text(page, ".job-location .location-address, .company-address .job-location")
        if addr:
            item["company_address"] = addr.strip()[:160]
