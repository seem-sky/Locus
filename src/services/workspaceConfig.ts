import { ipcInvoke } from "./ipc";
import type { WorkspaceBrowseFilters } from "../composables/useWorkspaceBrowseFilters";

export function getWorkspaceBrowseFilters(): Promise<WorkspaceBrowseFilters> {
  return ipcInvoke<WorkspaceBrowseFilters>("get_workspace_browse_filters");
}

export function setWorkspaceBrowseFilters(
  browseFilters: WorkspaceBrowseFilters,
): Promise<void> {
  return ipcInvoke<void>("set_workspace_browse_filters", { browseFilters });
}
