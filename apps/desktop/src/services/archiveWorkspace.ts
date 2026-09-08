export type ArchivePanelState = {
  categoryId: string;
  collapsed: boolean;
};

export function selectCategoryPanel(state: ArchivePanelState, categoryId: string): ArchivePanelState {
  return {
    categoryId,
    collapsed: state.categoryId === categoryId ? !state.collapsed : false,
  };
}

export function selectArchivePanelCategory(categoryId?: string): ArchivePanelState {
  return { categoryId: categoryId ?? "all", collapsed: false };
}
