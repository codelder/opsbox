<script lang="ts">
  // 设置按钮组件，点击跳转到设置页面
  function handleClick() {
    window.location.href = '/settings';
  }
</script>

<!-- 固定位置的设置按钮 -->
<button
  type="button"
  aria-label="打开设置"
  class="inline-flex h-10 w-10 items-center justify-center rounded-full bg-white/80 text-gray-900 shadow-sm backdrop-blur select-none hover:bg-white focus:ring-2 focus:ring-blue-500 focus:outline-none dark:bg-gray-800/80 dark:text-gray-100 dark:hover:bg-gray-800"
  onclick={handleClick}
>
  <svg
    xmlns="http://www.w3.org/2000/svg"
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    stroke-width="1.5"
    class="h-[1.2rem] w-[1.2rem]"
  >
    <path
      stroke-linecap="round"
      stroke-linejoin="round"
      d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 0 0 2.573 1.066c1.543-.89 3.31.876 2.42 2.42a1.724 1.724 0 0 0 1.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 0 0-1.066 2.573c.89 1.543-.876 3.31-2.42 2.42a1.724 1.724 0 0 0-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 0 0-2.573-1.066c-1.543.89-3.31-.876-2.42-2.42a1.724 1.724 0 0 0-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 0 0 1.066-2.573c-.89-1.543.876-3.31 2.42-2.42.996.575 2.245.021 2.572-1.065z"
    />
    <path stroke-linecap="round" stroke-linejoin="round" d="M15 12a3 3 0 1 1-6 0 3 3 0 0 1 6 0z" />
  </svg>
</button>
