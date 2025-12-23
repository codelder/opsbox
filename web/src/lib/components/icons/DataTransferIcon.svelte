<script lang="ts">
  import SourceAgentIcon from './SourceAgentIcon.svelte';
  import SourceS3Icon from './SourceS3Icon.svelte';
  import SourceLocalIcon from './SourceLocalIcon.svelte';
  import TargetTarIcon from './TargetTarIcon.svelte';
  import TargetTarGzIcon from './TargetTarGzIcon.svelte';
  import TargetGzIcon from './TargetGzIcon.svelte';
  import TargetDirIcon from './TargetDirIcon.svelte';
  import TargetFilesIcon from './TargetFilesIcon.svelte';

  interface Props {
    source: 'agent' | 's3' | 'local';
    target: 'tar' | 'targz' | 'gz' | 'dir' | 'files';
    size?: number;
    class?: string;
  }
  let { source, target, size = 24, class: className = '' }: Props = $props();

  const sourceIcons = {
    agent: SourceAgentIcon,
    s3: SourceS3Icon,
    local: SourceLocalIcon
  };

  const targetIcons = {
    tar: TargetTarIcon,
    targz: TargetTarGzIcon,
    gz: TargetGzIcon,
    dir: TargetDirIcon,
    files: TargetFilesIcon
  };

  const SourceCmp = $derived(sourceIcons[source]);
  const TargetCmp = $derived(targetIcons[target]);
</script>

<div class="inline-flex items-center gap-1.5 {className}" style="font-size: {size}px;">
  <div class="text-blue-600 dark:text-blue-400">
    <SourceCmp {size} />
  </div>

  <!-- Arrow -->
  <svg
    xmlns="http://www.w3.org/2000/svg"
    width={size * 0.6}
    height={size * 0.6}
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    stroke-width="2"
    stroke-linecap="round"
    stroke-linejoin="round"
    class="text-gray-400"
  >
    <path d="M5 12h14" />
    <path d="m12 5 7 7-7 7" />
  </svg>

  <div class="text-purple-600 dark:text-purple-400">
    <TargetCmp {size} />
  </div>
</div>
