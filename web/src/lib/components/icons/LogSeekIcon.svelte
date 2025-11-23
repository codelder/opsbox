<script lang="ts">
  interface Props {
    size?: number;
    class?: string;
  }
  let { size = 24, class: className = '' }: Props = $props();
</script>

<svg
  xmlns="http://www.w3.org/2000/svg"
  width={size}
  height={size}
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width="2"
  stroke-linecap="round"
  stroke-linejoin="round"
  class={className}
>
  <!-- Document outline -->
  <path d="M14.5 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7.5L14.5 2z" />
  <polyline points="14 2 14 8 20 8" />
  
  <!-- Log lines -->
  <line x1="8" y1="13" x2="16" y2="13" />
  <line x1="8" y1="17" x2="12" y2="17" />
  
  <!-- Magnifying glass (Seek) -->
  <path d="m21 21-4.3-4.3" />
  <circle cx="10" cy="10" r="4" />
</svg>
