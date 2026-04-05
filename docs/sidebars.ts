import type {SidebarsConfig} from '@docusaurus/plugin-content-docs';

const sidebars: SidebarsConfig = {
  architectureSidebar: [
    {
      type: 'category',
      label: '全局架构',
      collapsed: false,
      items: [
        'overview/intro',
        'overview/architecture',
        'overview/data-flow',
        'overview/design-highlights',
      ],
    },
    {
      type: 'category',
      label: '根文件',
      items: [
        'root-files/main-entry',
        'root-files/query-engine',
        'root-files/tool-framework',
        'root-files/commands-registry',
        'root-files/setup-and-cost',
      ],
    },
    {
      type: 'category',
      label: 'tools/ — 工具系统',
      items: [
        'tools/bash-tool',
        'tools/agent-tool',
        'tools/file-tools',
        'tools/search-tools',
        'tools/mcp-tools',
        'tools/task-tools',
        'tools/plan-and-worktree',
        'tools/other-tools',
      ],
    },
    {
      type: 'category',
      label: 'services/ — 服务层',
      items: [
        'services/api',
        'services/mcp',
        'services/compact',
        'services/analytics',
        'services/oauth-and-plugins',
        'services/other-services',
      ],
    },
    {
      type: 'category',
      label: 'utils/ — 工具函数',
      items: [
        'utils/bash-security',
        'utils/permissions',
        'utils/hooks-utils',
        'utils/messages',
        'utils/auth-and-shell',
        'utils/other-utils',
      ],
    },
    {
      type: 'category',
      label: 'Agent 核心',
      items: [
        'bootstrap/index',
        'bridge/index',
        'coordinator/index',
        'memdir/index',
        'tasks/index',
        'skills/index',
        'remote/index',
        'server/index',
      ],
    },
    {
      type: 'category',
      label: 'UI 与状态',
      items: [
        'hooks/index',
        'state/index',
        'ink/index',
        'commands/index',
        'components/index',
        'context/index',
        'screens/index',
      ],
    },
    {
      type: 'category',
      label: '其他目录',
      items: [
        'assistant/index',
        'buddy/index',
        'cli/index',
        'constants/index',
        'entrypoints/index',
        'keybindings/index',
        'migrations/index',
        'plugins/index',
        'query/index',
        'schemas/index',
        'types/index',
        'misc/vim-and-voice',
        'misc/proxy-and-native',
      ],
    },
  ],
};

export default sidebars;
