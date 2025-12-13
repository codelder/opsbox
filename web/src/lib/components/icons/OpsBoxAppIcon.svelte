<script lang="ts">
  interface Props {
    size?: number;
    class?: string;
  }
  let { size = 64, class: className = '' }: Props = $props();
</script>

<svg
  xmlns="http://www.w3.org/2000/svg"
  width={size}
  height={size}
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width="1.5"
  stroke-linecap="round"
  stroke-linejoin="round"
  class={className}
>
  <!-- Toolbox Body -->
  <path d="M3 9a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V9Z" />

  <!-- Handle -->
  <path d="M8 7V5a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2" />

  <!-- Latch -->
  <path d="M10 9v2" />
  <path d="M14 9v2" />

  <!-- Tools sticking out -->
  <!-- Wrench head -->
  <path d="m18.5 5.5-2 2" />
  <path d="m20.5 3.5-2 2" />
  <path d="M19 3a2 2 0 0 1 2 2l-2 2a2 2 0 0 1-2-2l2-2Z" />

  <!-- Screwdriver handle -->
  <path d="m5 5 2 2" />
  <path d="m4 4 2 2" />

  <!-- Hammer head hint -->
  <path d="M12 4v3" />
  <path d="M10 4h4" />
</svg>
