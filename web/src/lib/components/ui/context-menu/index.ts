import { ContextMenu as ContextMenuPrimitive } from 'bits-ui';

import Content from './context-menu-content.svelte';
import Item from './context-menu-item.svelte';
import Separator from './context-menu-separator.svelte';

const Root = ContextMenuPrimitive.Root;
const Trigger = ContextMenuPrimitive.Trigger;
const Group = ContextMenuPrimitive.Group;

export {
  Root,
  Trigger,
  Group,
  Content,
  Item,
  Separator,
  //
  Root as ContextMenu,
  Trigger as ContextMenuTrigger,
  Group as ContextMenuGroup,
  Content as ContextMenuContent,
  Item as ContextMenuItem,
  Separator as ContextMenuSeparator
};
