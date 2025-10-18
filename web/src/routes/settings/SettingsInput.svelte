<script lang="ts">
  /**
   * 设置页面输入框组件
   * 统一的输入框样式和布局
   */
  interface Props {
    /**
     * 字段标签
     */
    label: string;
    /**
     * 字段说明
     */
    description: string;
    /**
     * 占位符
     */
    placeholder: string;
    /**
     * 输入类型
     */
    type?: 'text' | 'password';
    /**
     * 是否禁用
     */
    disabled?: boolean;
    /**
     * 输入值
     */
    value: string;
    /**
     * 自动完成属性
     */
    autocomplete?: HTMLInputElement['autocomplete'];
    /**
     * 值变化回调
     */
    onInput?: (value: string) => void;
  }

  let {
    label,
    description,
    placeholder,
    type = 'text',
    disabled = false,
    value = $bindable(''),
    autocomplete,
    onInput
  }: Props = $props();

  function handleInput(e: Event) {
    const target = e.target as HTMLInputElement;
    value = target.value;
    onInput?.(target.value);
  }
</script>

<label
  class="flex flex-col gap-3 rounded-2xl border border-transparent bg-white/60 p-4 text-sm text-slate-500 shadow-sm shadow-slate-200/40 transition hover:border-slate-200 hover:shadow-slate-200 dark:bg-slate-900/60 dark:text-slate-400 dark:hover:border-slate-700"
>
  <span>
    <span class="block text-xs font-semibold tracking-[0.2em] text-indigo-500 uppercase dark:text-indigo-400">
      {label}
    </span>
    <span class="block text-sm leading-relaxed text-slate-600 dark:text-slate-300">
      {description}
    </span>
  </span>
  <input
    class="w-full rounded-xl border border-slate-200 bg-white px-3 py-3 text-sm text-slate-900 shadow-inner shadow-slate-200 focus:border-indigo-500 focus:ring-4 focus:ring-indigo-100 focus:outline-none dark:border-slate-700 dark:bg-slate-950 dark:text-slate-100 dark:shadow-none dark:focus:border-indigo-400 dark:focus:ring-indigo-500/30"
    {placeholder}
    {type}
    {disabled}
    {autocomplete}
    {value}
    oninput={handleInput}
  />
</label>
