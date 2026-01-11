import { test, expect } from '@playwright/test';

test.describe('Explorer Interaction E2E (Mocked)', () => {
  test.beforeEach(async ({ page }) => {
    // 拦截 Explorer List 请求
    await page.route('**/api/v1/explorer/list', async (route) => {
      const body = route.request().postDataJSON();
      // 标准化路径，移除结尾斜杠进行匹配
      const orl = (body.orl || '').replace(/\/$/, '');

      let items: any[] = [];
      if (orl === 'orl://local' || orl === '') {
        items = [{ name: 'Users', path: 'orl://local/Users/', type: 'dir' }];
      } else if (orl === 'orl://local/Users') {
        items = [{ name: 'testuser', path: 'orl://local/Users/testuser/', type: 'dir' }];
      } else if (orl === 'orl://local/Users/testuser') {
        items = [
          { name: 'logs', path: 'orl://local/Users/testuser/logs/', type: 'dir' },
          {
            name: 'archive.tar',
            path: 'orl://local/Users/testuser/archive.tar',
            type: 'file',
            mime_type: 'application/x-tar'
          }
        ];
      } else if (orl === 'orl://local/Users/testuser/logs') {
        items = [
          { name: 'app.log', path: 'orl://local/Users/testuser/logs/app.log', type: 'file', mime_type: 'text/plain' }
        ];
      } else if (orl.includes('archive.tar')) {
        items = [
          { name: 'internal.log', path: orl + (orl.includes('?') ? '&' : '?') + 'entry=internal.log', type: 'file' }
        ];
      }

      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({ data: { items } })
      });
    });

    await page.goto('/explorer');
  });

  test('should navigate deep and view files via mock', async ({ page }) => {
    // 1. 从根目录逐层双击
    await expect(page.getByText('Users', { exact: true })).toBeVisible();
    await page.getByText('Users', { exact: true }).dblclick();

    await expect(page.getByText('testuser', { exact: true })).toBeVisible();
    await page.getByText('testuser', { exact: true }).dblclick();

    await expect(page.getByText('logs', { exact: true })).toBeVisible();
    await page.getByText('logs', { exact: true }).dblclick();

    await expect(page.getByText('app.log', { exact: true })).toBeVisible();

    // 2. 点击回退按钮
    await page
      .getByRole('button')
      .filter({ has: page.locator('.lucide-arrow-left') })
      .click();

    // 等待列表内容更新，看到 archive.tar
    await expect(page.getByText('archive.tar', { exact: true })).toBeVisible();

    // 3. 点击进入归档
    await page.getByText('archive.tar', { exact: true }).dblclick();
    await expect(page.getByText('internal.log', { exact: true })).toBeVisible();
  });

  test('should navigate via manual ORL input entry', async ({ page }) => {
    const input = page.locator('#orl-input');
    await input.fill('orl://local/Users');
    await input.press('Enter');

    // URL should update and content should show testuser
    await expect(page).toHaveURL(/\/explorer\?orl=orl%3A%2F%2Flocal%2FUsers/);
    await expect(page.getByText('testuser', { exact: true })).toBeVisible();
  });

  test('should toggle hidden files visibility', async ({ page }) => {
    // Setup mock for hidden files
    await page.route('**/api/v1/explorer/list', async (route) => {
      const body = route.request().postDataJSON();
      if (body.orl.includes('hidden_test')) {
        await route.fulfill({
          status: 200,
          contentType: 'application/json',
          body: JSON.stringify({
            data: {
              items: [
                { name: '.hidden_file', path: 'orl://local/hidden_test/.hidden_file', type: 'file' },
                { name: 'visible_file', path: 'orl://local/hidden_test/visible_file', type: 'file' }
              ]
            }
          })
        });
      }
    });

    await page.goto('/explorer?orl=' + encodeURIComponent('orl://local/hidden_test/'));

    // Initially .hidden_file should NOT be visible (default showHidden=false)
    await expect(page.getByText('visible_file')).toBeVisible();
    await expect(page.getByText('.hidden_file')).not.toBeVisible();

    // Click toggle button
    await page.getByTitle(/Show hidden files/i).click();

    // Now .hidden_file should be visible
    await expect(page.getByText('.hidden_file')).toBeVisible();
  });

  test('should switch between table and grid view modes', async ({ page }) => {
    // Default is grid
    await expect(page.locator('table')).not.toBeVisible();
    await expect(page.getByText('Users', { exact: true })).toBeVisible();

    // Switch to table
    await page
      .locator('button')
      .filter({ has: page.locator('.lucide-layout-list') })
      .click();

    // Table should be visible
    await expect(page.locator('table')).toBeVisible();

    // Switch back to grid
    await page
      .locator('button')
      .filter({ has: page.locator('.lucide-layout-grid') })
      .click();
    await expect(page.locator('table')).not.toBeVisible();
  });

  test('should navigate using sidebar links', async ({ page }) => {
    // Click "Remote Agents" in sidebar
    await page.getByText('Remote Agents').click();
    await expect(page).toHaveURL(/\/explorer\?orl=orl%3A%2F%2Fagent%2F/);

    // Click "S3 Storage"
    await page.getByText('S3 Storage').click();
    await expect(page).toHaveURL(/\/explorer\?orl=orl%3A%2F%2Fs3%2F/);

    // Click "Local Machine"
    await page.getByText('Local Machine').click();
    await expect(page).toHaveURL(/\/explorer\?orl=orl%3A%2F%2Flocal%2F/);
  });

  test('should open viewer on double click file', async ({ page }) => {
    await page.goto('/explorer?orl=' + encodeURIComponent('orl://local/Users/testuser/logs/'));
    await expect(page.getByText('app.log', { exact: true })).toBeVisible();

    const [newPage] = await Promise.all([
      page.waitForEvent('popup'),
      page.getByText('app.log', { exact: true }).dblclick()
    ]);
    await expect(newPage).toHaveURL(/\/view\?/);
  });

  test('should display right-click menu for file', async ({ page }) => {
    await page.goto('/explorer?orl=' + encodeURIComponent('orl://local/Users/testuser/logs/'));

    // Wait for file to appear
    const fileItem = page.getByText('app.log', { exact: true });
    await expect(fileItem).toBeVisible();

    // Right click
    await fileItem.click({ button: 'right' });

    // Verify menu items
    await expect(page.getByText('复制 ORL 路径')).toBeVisible();
    await expect(page.getByText('下载')).toBeVisible();
  });

  test('should refresh list via toolbar button', async ({ page }) => {
    let requestCount = 0;
    await page.route('**/api/v1/explorer/list', async (route) => {
      requestCount++;
      await route.continue();
    });

    await page.goto('/explorer?orl=orl%3A%2F%2Flocal%2F');
    const initialCount = requestCount;

    // Click refresh in toolbar
    await page.getByTitle('刷新').click();

    // Expect at least one more request
    expect(requestCount).toBeGreaterThan(initialCount);
  });

  test('should refresh list via container context menu', async ({ page }) => {
    await page.goto('/explorer?orl=orl%3A%2F%2Flocal%2F');

    let refreshed = false;
    await page.route('**/api/v1/explorer/list', async (route) => {
      refreshed = true;
      await route.continue();
    });

    // Right click on background (content area)
    await page.getByTestId('explorer-content').click({ button: 'right', position: { x: 10, y: 10 } });

    // Click "Refresh" in context menu
    await page.getByRole('menuitem', { name: '刷新' }).click();

    expect(refreshed).toBe(true);
  });

  test('should manage item selection and clear on background click', async ({ page }) => {
    await page.goto('/explorer?orl=' + encodeURIComponent('orl://local/Users/testuser/logs/'));

    // Switch to grid mode and wait
    await page
      .locator('button')
      .filter({ has: page.locator('.lucide-layout-grid') })
      .click();
    await page.waitForTimeout(500);

    // Click text to select
    const fileItem = page.getByText('app.log', { exact: true });
    await fileItem.click();

    // In grid mode, the selected item's text becomes white
    await expect(page.locator('.text-white').filter({ hasText: 'app.log' }).first()).toBeVisible();

    // Click background to clear (top left of content area)
    await page.getByTestId('explorer-content').click({ position: { x: 5, y: 5 } });

    // Selection should be cleared
    await expect(page.locator('.text-white').filter({ hasText: 'app.log' })).not.toBeVisible();
  });

  test('should display visual error state on API failure', async ({ page }) => {
    // Mock a 500 error
    await page.route('**/api/v1/explorer/list', (route) => {
      route.fulfill({
        status: 500,
        body: JSON.stringify({ error: 'Database connection failed' })
      });
    });

    await page.goto('/explorer?orl=orl%3A%2F%2Flocal%2Ferror_path');

    // Should see error illustration and retry button
    await expect(page.getByText('资源列举失败')).toBeVisible();
    await expect(page.getByText('Database connection failed')).toBeVisible();
    await expect(page.getByRole('button', { name: '重试', exact: true })).toBeVisible();
  });
});
