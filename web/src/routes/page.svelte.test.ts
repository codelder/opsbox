/**
 * Home Page 单元测试
 *
 * 测试页面元素渲染：
 * - 搜索输入框
 * - AI 模式按钮
 * - 语法提示按钮
 * - 导航按钮
 */

import { describe, it, expect } from 'vitest';
import { render } from 'vitest-browser-svelte';
import { page } from '@vitest/browser/context';
import Home from './+page.svelte';

describe('Home Page', () => {
  it('should render search input', async () => {
    render(Home, {});

    // textarea has id="search" and aria-labelledby="logo-label"
    const searchInput = page.getByPlaceholder(/试一下/);
    await expect.element(searchInput).toBeInTheDocument();
  });

  it('should render AI mode button', async () => {
    render(Home, {});

    const aiButton = page.getByRole('button', { name: /AI 模式|AI mode/i });
    await expect.element(aiButton).toBeInTheDocument();
  });

  it('should render syntax hint buttons', async () => {
    render(Home, {});

    const orButton = page.getByRole('button', { name: 'OR', exact: true });
    const andButton = page.getByRole('button', { name: 'AND', exact: true });

    await expect.element(orButton).toBeInTheDocument();
    await expect.element(andButton).toBeInTheDocument();
  });

  it('should render navigation buttons', async () => {
    render(Home, {});

    const settingsButton = page.getByRole('button', { name: /打开设置|settings/i });
    const themeButton = page.getByRole('button', { name: /toggle theme|主题/i });
    const exampleButton = page.getByRole('button', { name: /示例|example/i });
    const promptButton = page.getByRole('button', { name: /系统提示词|prompt/i });

    await expect.element(settingsButton).toBeInTheDocument();
    await expect.element(themeButton).toBeInTheDocument();
    await expect.element(exampleButton).toBeInTheDocument();
    await expect.element(promptButton).toBeInTheDocument();
  });
});
