<script lang="ts">
  /**
   * 设置页面输入框组件
   * 统一的输入框样式和布局
   */
  import { Input } from "$lib/components/ui/input";
  import { Label } from "$lib/components/ui/label";

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

<div class="grid gap-2">
  <Label class="text-base font-semibold">{label}</Label>
  <p class="text-sm text-muted-foreground">
    {description}
  </p>
  <Input
    {placeholder}
    {type}
    {disabled}
    {autocomplete}
    bind:value
    oninput={handleInput}
    class="max-w-md"
  />
</div>
