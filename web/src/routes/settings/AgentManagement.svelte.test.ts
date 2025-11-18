/**
 * AgentManagement 组件测试
 * 测试 Agent 管理组件的日志设置功能
 */
import { expect, test, vi, beforeEach, afterEach } from 'vitest';
import { render } from 'vitest-browser-svelte';
import { page, userEvent } from '@vitest/browser/context';
import type { AgentInfo } from '$lib/modules/agent';
import type { LogConfigResponse } from '$lib/modules/agent/api';
import AgentManagement from './AgentManagement.svelte';

// Mock API 模块
vi.mock('$lib/modules/agent', () => ({
  useAgents: vi.fn()
}));

vi.mock('$lib/modules/agent/api', () => ({
  fetchAgentLogConfig: vi.fn(),
  updateAgentLogLevel: vi.fn(),
  updateAgentLogRetention: vi.fn()
}));

// Mock Alert 组件
vi.mock('$lib/components/Alert.svelte', () => ({
  default: vi.fn(() => ({
    render: () => '<div data-testid="alert"></div>'
  }))
}));

import { useAgents } from '$lib/modules/agent';
import {
  fetchAgentLogConfig,
  updateAgentLogLevel,
  updateAgentLogRetention
} from '$lib/modules/agent/api';

const mockAgentsStore = {
  agents: [] as AgentInfo[],
  total: 0,
  loading: false,
  error: null,
  tagFilter: '',
  onlineOnly: false,
  load: vi.fn(),
  addTag: vi.fn(),
  removeTag: vi.fn()
};

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(useAgents).mockReturnValue(mockAgentsStore);
});

afterEach(() => {
  vi.restoreAllMocks();
});

test('组件渲染 - 显示 Agent 列表', async () => {
  const mockAgents: AgentInfo[] = [
    {
      id: 'agent-1',
      name: 'Test Agent 1',
      hostname: 'host1',
      version: '1.0.0',
      last_heartbeat: Math.floor(Date.now() / 1000),
      status: { type: 'Online' },
      tags: [{ key: 'env', value: 'production' }],
      search_roots: ['/var/log']
    }
  ];

  mockAgentsStore.agents = mockAgents;
  mockAgentsStore.total = 1;

  render(AgentManagement, {});

  const agentName = await page.getByText('Test Agent 1');
  expect.element(agentName).toBeInTheDocument();

  const hostname = await page.getByText(/host1/);
  expect.element(hostname).toBeInTheDocument();
});

test('组件渲染 - 显示空状态', async () => {
  mockAgentsStore.agents = [];
  mockAgentsStore.total = 0;

  render(AgentManagement, {});

  const emptyState = await page.getByText('暂无数据');
  expect.element(emptyState).toBeInTheDocument();
});

test('日志设置 - 展开日志设置区域', async () => {
  const mockAgents: AgentInfo[] = [
    {
      id: 'agent-1',
      name: 'Test Agent 1',
      hostname: 'host1',
      version: '1.0.0',
      last_heartbeat: Math.floor(Date.now() / 1000),
      status: { type: 'Online' },
      tags: [],
      search_roots: ['/var/log']
    }
  ];

  mockAgentsStore.agents = mockAgents;
  mockAgentsStore.total = 1;

  const mockLogConfig = {
    level: 'info',
    retention_count: 7,
    log_dir: '/home/user/.opsbox-agent/logs'
  };

  vi.mocked(fetchAgentLogConfig).mockResolvedValue(mockLogConfig);

  render(AgentManagement, {});

  const logSettingsButton = await page.getByRole('button', { name: /日志设置/ });
  await userEvent.click(logSettingsButton);

  await vi.waitFor(async () => {
    const levelLabel = await page.getByText('日志级别');
    expect.element(levelLabel).toBeInTheDocument();
  });

  expect(fetchAgentLogConfig).toHaveBeenCalledWith('agent-1');
});

test('日志设置 - 修改日志级别', async () => {
  const mockAgents: AgentInfo[] = [
    {
      id: 'agent-1',
      name: 'Test Agent 1',
      hostname: 'host1',
      version: '1.0.0',
      last_heartbeat: Math.floor(Date.now() / 1000),
      status: { type: 'Online' },
      tags: [],
      search_roots: ['/var/log']
    }
  ];

  mockAgentsStore.agents = mockAgents;
  mockAgentsStore.total = 1;

  const mockLogConfig = {
    level: 'info',
    retention_count: 7,
    log_dir: '/home/user/.opsbox-agent/logs'
  };

  vi.mocked(fetchAgentLogConfig).mockResolvedValue(mockLogConfig);

  render(AgentManagement, {});

  const logSettingsButton = await page.getByRole('button', { name: /日志设置/ });
  await userEvent.click(logSettingsButton);

  await vi.waitFor(async () => {
    const levelSelect = await page.getByRole('combobox');
    expect.element(levelSelect).toBeInTheDocument();
  });

  const levelSelect = page.getByRole('combobox');
  const element = await levelSelect.element();
  await userEvent.selectOptions(element, 'debug');

  await expect.element(levelSelect).toHaveValue('debug');
});

test('日志设置 - 保存配置成功', async () => {
  const mockAgents: AgentInfo[] = [
    {
      id: 'agent-1',
      name: 'Test Agent 1',
      hostname: 'host1',
      version: '1.0.0',
      last_heartbeat: Math.floor(Date.now() / 1000),
      status: { type: 'Online' },
      tags: [],
      search_roots: ['/var/log']
    }
  ];

  mockAgentsStore.agents = mockAgents;
  mockAgentsStore.total = 1;

  const mockLogConfig = {
    level: 'info',
    retention_count: 7,
    log_dir: '/home/user/.opsbox-agent/logs'
  };

  vi.mocked(fetchAgentLogConfig).mockResolvedValue(mockLogConfig);
  vi.mocked(updateAgentLogLevel).mockResolvedValue({} as never);
  vi.mocked(updateAgentLogRetention).mockResolvedValue({} as never);

  render(AgentManagement, {});

  const logSettingsButton = await page.getByRole('button', { name: /日志设置/ });
  await userEvent.click(logSettingsButton);

  await vi.waitFor(async () => {
    const saveButton = await page.getByRole('button', { name: /保存/ });
    expect.element(saveButton).toBeInTheDocument();
  });

  const saveButton = await page.getByRole('button', { name: /保存/ });
  await userEvent.click(saveButton);

  await vi.waitFor(() => {
    expect(updateAgentLogLevel).toHaveBeenCalledWith('agent-1', 'info');
    expect(updateAgentLogRetention).toHaveBeenCalledWith('agent-1', 7);
  });
});

test('日志设置 - Agent 离线时禁用表单', async () => {
  const mockAgents: AgentInfo[] = [
    {
      id: 'agent-1',
      name: 'Test Agent 1',
      hostname: 'host1',
      version: '1.0.0',
      last_heartbeat: Math.floor(Date.now() / 1000) - 3600,
      status: { type: 'Offline' },
      tags: [],
      search_roots: ['/var/log']
    }
  ];

  mockAgentsStore.agents = mockAgents;
  mockAgentsStore.total = 1;

  const mockLogConfig = {
    level: 'info',
    retention_count: 7,
    log_dir: '/home/user/.opsbox-agent/logs'
  };

  vi.mocked(fetchAgentLogConfig).mockResolvedValue(mockLogConfig);

  render(AgentManagement, {});

  const logSettingsButton = await page.getByRole('button', { name: /日志设置/ });
  await userEvent.click(logSettingsButton);

  await vi.waitFor(async () => {
    const levelSelect = await page.getByRole('combobox');
    expect.element(levelSelect).toBeDisabled();
  });

  const warningText = await page.getByText(/Agent 离线，无法修改配置/);
  expect.element(warningText).toBeInTheDocument();
});

test('日志设置 - 加载配置失败显示错误', async () => {
  const mockAgents: AgentInfo[] = [
    {
      id: 'agent-1',
      name: 'Test Agent 1',
      hostname: 'host1',
      version: '1.0.0',
      last_heartbeat: Math.floor(Date.now() / 1000),
      status: { type: 'Online' },
      tags: [],
      search_roots: ['/var/log']
    }
  ];

  mockAgentsStore.agents = mockAgents;
  mockAgentsStore.total = 1;

  vi.mocked(fetchAgentLogConfig).mockRejectedValue(new Error('加载日志配置失败'));

  render(AgentManagement, {});

  const logSettingsButton = await page.getByRole('button', { name: /日志设置/ });
  await userEvent.click(logSettingsButton);

  await vi.waitFor(async () => {
    const errorMessage = await page.getByText('加载日志配置失败');
    expect.element(errorMessage).toBeInTheDocument();
  });
});

test('日志设置 - 保存配置失败显示错误', async () => {
  const mockAgents: AgentInfo[] = [
    {
      id: 'agent-1',
      name: 'Test Agent 1',
      hostname: 'host1',
      version: '1.0.0',
      last_heartbeat: Math.floor(Date.now() / 1000),
      status: { type: 'Online' },
      tags: [],
      search_roots: ['/var/log']
    }
  ];

  mockAgentsStore.agents = mockAgents;
  mockAgentsStore.total = 1;

  const mockLogConfig = {
    level: 'info',
    retention_count: 7,
    log_dir: '/home/user/.opsbox-agent/logs'
  };

  vi.mocked(fetchAgentLogConfig).mockResolvedValue(mockLogConfig);
  vi.mocked(updateAgentLogLevel).mockRejectedValue(new Error('保存失败'));

  render(AgentManagement, {});

  const logSettingsButton = await page.getByRole('button', { name: /日志设置/ });
  await userEvent.click(logSettingsButton);

  await vi.waitFor(async () => {
    const saveButton = await page.getByRole('button', { name: /保存/ });
    expect.element(saveButton).toBeInTheDocument();
  });

  const saveButton = await page.getByRole('button', { name: /保存/ });
  await userEvent.click(saveButton);

  await vi.waitFor(async () => {
    const errorMessage = await page.getByText('保存失败');
    expect.element(errorMessage).toBeInTheDocument();
  });
});

test('标签管理 - 添加标签', async () => {
  const mockAgents: AgentInfo[] = [
    {
      id: 'agent-1',
      name: 'Test Agent 1',
      hostname: 'host1',
      version: '1.0.0',
      last_heartbeat: Math.floor(Date.now() / 1000),
      status: { type: 'Online' },
      tags: [],
      search_roots: ['/var/log']
    }
  ];

  mockAgentsStore.agents = mockAgents;
  mockAgentsStore.total = 1;

  render(AgentManagement, {});

  const keyInput = await page.getByPlaceholder('key');
  const valueInput = await page.getByPlaceholder('value');
  const addButton = await page.getByRole('button', { name: /添加标签/ });

  await userEvent.type(keyInput, 'env');
  await userEvent.type(valueInput, 'production');
  await userEvent.click(addButton);

  expect(mockAgentsStore.addTag).toHaveBeenCalledWith('agent-1', 'env', 'production');
});

test('标签管理 - 移除标签', async () => {
  const mockAgents: AgentInfo[] = [
    {
      id: 'agent-1',
      name: 'Test Agent 1',
      hostname: 'host1',
      version: '1.0.0',
      last_heartbeat: Math.floor(Date.now() / 1000),
      status: { type: 'Online' },
      tags: [{ key: 'env', value: 'production' }],
      search_roots: ['/var/log']
    }
  ];

  mockAgentsStore.agents = mockAgents;
  mockAgentsStore.total = 1;

  render(AgentManagement, {});

  const removeButton = await page.getByTitle('移除标签');
  await userEvent.click(removeButton);

  expect(mockAgentsStore.removeTag).toHaveBeenCalledWith('agent-1', 'env', 'production');
});

test('过滤功能 - 标签筛选', async () => {
  mockAgentsStore.agents = [];
  mockAgentsStore.total = 0;

  render(AgentManagement, {});

  const filterInput = await page.getByPlaceholder(/key=value/);
  await userEvent.type(filterInput, 'env=production');

  expect(mockAgentsStore.tagFilter).toBe('env=production');
});

test('过滤功能 - 只看在线', async () => {
  mockAgentsStore.agents = [];
  mockAgentsStore.total = 0;

  render(AgentManagement, {});

  const onlineOnlyCheckbox = await page.getByRole('checkbox', { name: /只看在线/ });
  await userEvent.click(onlineOnlyCheckbox);

  expect(mockAgentsStore.onlineOnly).toBe(true);
  expect(mockAgentsStore.load).toHaveBeenCalled();
});
