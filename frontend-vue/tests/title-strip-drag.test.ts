// @vitest-environment jsdom
/**
 * 标题带拖动区（用户裁决：只有窗口最上方 20px 能拖窗口 / 双击最大化）。
 *
 * 这里钉的是三条「写错就静默坏掉」的边界：
 * 1. 20px 以内、非控件 → 命中元素被补上 `data-tauri-drag-region`（Tauri 只看被点到的那个节点）；
 * 2. 20px 以外 → 一律不补，页面空白处不能拖窗口；
 * 3. 上一次的标记必须摘掉——属性留在节点上，那个节点会永久变成拖动区。
 * 双击最大化由 Tauri 自带 `drag.js` 处理，前端不重复发命令，本文件不测。
 */
import { beforeEach, describe, expect, it } from 'vitest';

import { DRAG_ATTR, TITLE_STRIP_HEIGHT, installDragAllow } from '../src/lib_ui/drag-allow';

let teardown: () => void;

function press(el: Element | null, clientY: number): void {
  if (el === null) throw new Error('测试节点没找到');
  el.dispatchEvent(new MouseEvent('pointerdown', { bubbles: true, button: 0, clientY }));
}

function move(el: Element): void {
  el.dispatchEvent(new MouseEvent('pointermove', { bubbles: true, clientY: 4 }));
}

function up(el: Element): void {
  el.dispatchEvent(new MouseEvent('pointerup', { bubbles: true, clientY: 4 }));
}

beforeEach(() => {
  document.body.innerHTML =
    '<div class="app-shell">'
    + '<header class="topbar"><span class="brand">AI 求职助手</span>'
    + '<button type="button" class="win-btn">×</button></header>'
    + '<div class="app-body"><p class="blank">空白处</p>'
    + '<div class="drawer"><span class="drawer-inner">浮层</span></div></div>'
    + '</div>';
  teardown = installDragAllow(document);
  return () => teardown();
});

describe('标题带拖动区', () => {
  it('标题带常量就是 20px', () => {
    expect(TITLE_STRIP_HEIGHT).toBe(20);
  });

  it('20px 以内的空白处：命中元素被补标记', () => {
    const blank = document.querySelector('.blank');
    press(blank, 8);
    expect(blank?.getAttribute(DRAG_ATTR)).toBe('true');
    up(blank as Element);
  });

  it('20px 以外：页面空白处不补标记，窗口不跟着跑', () => {
    const blank = document.querySelector('.blank');
    press(blank, 30);
    expect(blank?.hasAttribute(DRAG_ATTR)).toBe(false);
    expect(document.querySelectorAll(`[${DRAG_ATTR}]`)).toHaveLength(0);
    up(blank as Element);
  });

  it('标题带里的控件保持原行为（点按钮不通向拖动）', () => {
    const button = document.querySelector('.win-btn');
    press(button, 8);
    expect(button?.hasAttribute(DRAG_ATTR)).toBe(false);
    up(button as Element);
  });

  it('浮层本身不是拖动区', () => {
    const drawer = document.querySelector('.drawer');
    press(drawer, 8);
    expect(drawer?.hasAttribute(DRAG_ATTR)).toBe(false);
    up(drawer as Element);
  });

  it('上一次的标记会被摘掉，不会变成永久拖动区', () => {
    const blank = document.querySelector('.blank');
    const brand = document.querySelector('.brand');
    press(blank, 8);
    expect(blank?.getAttribute(DRAG_ATTR)).toBe('true');
    up(blank as Element);
    press(brand, 8);
    expect(blank?.hasAttribute(DRAG_ATTR)).toBe(false);
    expect(brand?.getAttribute(DRAG_ATTR)).toBe('true');
    // 拖过以后 pointerup 收不到（系统 modal loop），下一次按下要能重新起拖。
    move(brand as Element);
    press(blank, 8);
    press(blank, 8);
    expect(blank?.getAttribute(DRAG_ATTR)).toBe('true');
    expect(brand?.hasAttribute(DRAG_ATTR)).toBe(false);
  });

  it('拖动后第一次按下只用于结账，第二次才起拖', () => {
    const brand = document.querySelector('.brand');
    const blank = document.querySelector('.blank');
    press(brand, 8);
    move(brand as Element); // 真实拖动：收不到 pointerup
    press(blank, 8);
    expect(blank?.hasAttribute(DRAG_ATTR)).toBe(false);
    press(blank, 8);
    expect(blank?.getAttribute(DRAG_ATTR)).toBe('true');
  });
});