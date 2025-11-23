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
  <!-- Outer Cube/Hexagon -->
  <path d="M21 16V8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73l7 4a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16z" />
  
  <!-- Inner Y (Cube edges) -->
  <path d="M7.5 4.21l4.5 2.6 4.5-2.6" />
  <path d="M7.5 19.79l4.5-2.6 4.5 2.6" />
  <path d="M3.29 7 12 12l8.71-5" />
  <path d="M12 22V12" />
  
  <!-- Tech/Ops Details on faces -->
  <!-- Right Face: Server lines -->
  <path d="m16 14.5 3 1.7" />
  <path d="m16 17.5 3 1.7" />
  
  <!-- Left Face: Terminal Prompt -->
  <path d="m5 16 2.5 1.5" />
  <path d="m5 19 4-2.5" />
  
  <!-- Top Face: Pulse/Activity -->
  <circle cx="12" cy="7" r="1.5" fill="currentColor" />
</svg>
