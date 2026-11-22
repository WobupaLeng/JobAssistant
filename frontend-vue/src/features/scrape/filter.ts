/**
 * 抓取筛选的**最小骨架**：只承载「筛什么」，不碰 DOM、不发命令。
 *
 * 本轮只接城市（用户裁决：先接城市，其余筛选控件与 scrape_start 留给后续阶段）。
 * 形状刻意留成 ScrapeFilter { city: string } 单字段：后续阶段把薪资 / 经验 /
 * 学历 / 规模 / 阶段 / 职位类型 / 行业 / 关键词逐项加进来时，**只加字段不改已有字段**，
 * 页面与组件读同一个对象，不必重新布线。
 *
 * 两条硬约束：
 * 1. 城市是**单值** 9 位编码 string，与 lib_ui/options 的 CITY_DEFAULT、
 *    后端 scrape_defaults 同口径，不做多选、不做行列化；
 * 2. 本文件属于 features 层，**不得** import components / pages（会成环），
 *    也不得 import 任何命令封装——真正的抓取请求在阶段 3 才接。
 */

import { CITY_DEFAULT } from '../../lib_ui/options';

/** 抓取筛选状态。字段名即后续 set_scrape_defaults 的语义单位（驼峰由 api 层负责）。 */
export interface ScrapeFilter {
  /** 城市：9 位编码；CITY_DEFAULT 为深圳。 */
  city: string;
}

/**
 * 初始筛选：城市取 CITY_DEFAULT，与 Leptos 版首屏一致。
 *
 * 每次调用都返回**新对象**，不要在模块级共享同一个字面量——页面之间来回切时
 * 共享对象会把上一页的改动漏到下一页。
 */
export function createFilter(): ScrapeFilter {
  return { city: CITY_DEFAULT };
}

/**
 * 是否仍是初始筛选（无任何改动）。
 *
 * 阶段 3 接 scrape_start 时用它决定「要不要把筛选当参数带上」：全默认就不发，
 * 免得把默认值写进后端 scrape_defaults。
 */
export function isDefaultFilter(filter: ScrapeFilter): boolean {
  return filter.city === CITY_DEFAULT;
}

/**
 * 只保留真正要发给后端的键。
 *
 * 现在只有城市，所以等价于 { city }；写成函数是为了阶段 3 加字段时，
 * 「哪些字段进载荷」这件事有一处集中表述，而不是散在页面里。
 */
export function filterPayload(filter: ScrapeFilter): { city: string } {
  return { city: filter.city };
}
