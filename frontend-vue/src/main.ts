/**
 * Vue 入口。
 *
 * 顺序有讲究：
 * 1. `bootstrapTheme()` 先跑：它把 `data-theme` 打到 `<html>` 上。首帧那一刻
 *    `index.html` 的内联脚本已经打过一次，这里补的是「系统偏好在首帧后才变」
 *    与「storage 被禁用」两种情况；两处判据必须一致，否则会闪一帧。
 * 2. 样式表 `styles/index.css` 只在这里 import 一次——它内部按 C4 的顺序
 *    `@import` 那 11 份分域 CSS，顺序就是层叠契约，别在别处再引。
 * 3. 最后才 `mount`：先有样式、先有主题，再有 DOM。
 */
import { createApp } from 'vue';

import { bootstrapTheme } from './lib_ui/theme';
import '../styles/index.css';
import './app.css';
import App from './App.vue';

bootstrapTheme();

createApp(App).mount('#app');
