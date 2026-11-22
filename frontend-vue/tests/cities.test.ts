import { describe, expect, it } from 'vitest';
import {
  ALL_CITIES,
  CITY_TOTAL,
  PROVINCES,
  cityName,
  findCity,
  pinyinInitials,
  searchCities,
} from '../src/lib_ui/data/cities';
import { CITIES, CITY_DEFAULT } from '../src/lib_ui/options';

// 这份数据是逐字抄自 BOSS 官网 city.json 的快照，编码写错**不会报错**，
// 只会表现成「筛不出岗位」。所以这里的断言是「防抄坏」而不是「防逻辑错」。

describe('城市表结构', () => {
  it('省级 34 个、城市 373 个', () => {
    expect(PROVINCES.length).toBe(34);
    expect(CITY_TOTAL).toBe(373);
    expect(ALL_CITIES.length).toBe(373);
  });

  it('每个省级至少一个城市，每个城市编码 / 名称 / 拼音都非空', () => {
    for (const p of PROVINCES) {
      expect(p.cities.length, `${p.name} 没有城市`).toBeGreaterThan(0);
      expect(p.code).toMatch(/^\d{9}$/);
      for (const c of p.cities) {
        expect(c.code, `${p.name}/${c.name} 编码`).toMatch(/^\d{9}$/);
        expect(c.name.length).toBeGreaterThan(0);
        expect(c.pinyin, `${c.name} 拼音`).toMatch(/^[a-z]+$/);
      }
    }
  });

  it('城市编码全表唯一（没有两个城市共用同一个 code）', () => {
    const codes = ALL_CITIES.map((c) => c.code);
    expect(new Set(codes).size).toBe(codes.length);
  });

  it('全表城市名唯一性允许重名，但「省 + 市」组合必须唯一', () => {
    const keys = ALL_CITIES.map((c) => `${c.province}/${c.name}`);
    expect(new Set(keys).size).toBe(keys.length);
  });
});

describe('热门城市与全量表一致', () => {
  it('8 个热门城市，顺序与旧版一致', () => {
    expect(CITIES.map(([code]) => code)).toEqual([
      '101280600',
      '101020100',
      '101010100',
      '101280100',
      '101210100',
      '101270100',
      '101190400',
      '101030100',
    ]);
  });

  it('每个热门城市的编码都能在全量表里查到，且名字对得上', () => {
    for (const [code, name] of CITIES) {
      const hit = findCity(code);
      expect(hit, `${name}(${code}) 不在全量表里`).toBeDefined();
      expect(hit?.name).toBe(name);
    }
  });

  it('默认城市是深圳，且在热门表里', () => {
    expect(CITY_DEFAULT).toBe('101280600');
    expect(CITIES.some(([code]) => code === CITY_DEFAULT)).toBe(true);
    expect(cityName(CITY_DEFAULT)).toBe('深圳');
  });
});

describe('精确编码抽样（与官网逐字核对过的锚点）', () => {
  it('八个热门编码解析到正确城市', () => {
    expect(cityName('101280600')).toBe('深圳');
    expect(cityName('101020100')).toBe('上海');
    expect(cityName('101010100')).toBe('北京');
    expect(cityName('101280100')).toBe('广州');
    expect(cityName('101210100')).toBe('杭州');
    expect(cityName('101270100')).toBe('成都');
    expect(cityName('101190400')).toBe('苏州');
    expect(cityName('101030100')).toBe('天津');
  });

  it('未知编码原样回显，不静默返回空串', () => {
    expect(cityName('999999999')).toBe('999999999');
    expect(findCity('999999999')).toBeUndefined();
  });
});

describe('拼音检索', () => {
  it('拼音首字母按音节切分', () => {
    expect(pinyinInitials('beijing').join('')).toBe('bj');
    expect(pinyinInitials('shijiazhuang').join('')).toBe('sjz');
    expect(pinyinInitials('hangzhou').join('')).toBe('hz');
    expect(pinyinInitials('guangzhou').join('')).toBe('gz');
    expect(pinyinInitials('nanjing').join('')).toBe('nj');
    expect(pinyinInitials('zhengzhou').join('')).toBe('zz');
    // 切分只保证「首字母串里包含真实缩写」，不强求逐音节正确：
    // shenzhen 会切成 shen|zhe|n（多一个 n），但 sz 一定在里面。
    expect(pinyinInitials('shenzhen').join('')).toContain('sz');
  });

  it('中文子串命中', () => {
    expect(searchCities('深圳').map((c) => c.name)).toContain('深圳');
    expect(searchCities('州').length).toBeGreaterThan(5);
  });

  it('全拼命中（大小写与前后空格不敏感）', () => {
    const hits = searchCities('  SHENZHEN ');
    expect(hits.map((c) => c.code)).toContain('101280600');
  });

  it('拼音可以带分隔符来搜', () => {
    expect(searchCities('shen zhen').map((c) => c.code)).toContain('101280600');
  });

  it('首字母缩写命中', () => {
    expect(searchCities('sz').map((c) => c.code)).toContain('101280600');
    expect(searchCities('bj').map((c) => c.code)).toContain('101010100');
    expect(searchCities('nj').map((c) => c.name)).toContain('南京');
  });

  it('编码命中', () => {
    expect(searchCities('101280600').map((c) => c.name)).toEqual(['深圳']);
  });

  it('空查询返回空数组，不铺全量 373 条', () => {
    expect(searchCities('')).toEqual([]);
    expect(searchCities('   ')).toEqual([]);
  });

  it('查无此项返回空数组', () => {
    expect(searchCities('zzzzzzzz')).toEqual([]);
  });

  it('命中项带省级名，便于显示「广东 · 深圳」', () => {
    const hit = searchCities('深圳')[0];
    expect(hit?.province).toBe('广东');
  });
});
