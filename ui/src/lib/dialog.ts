import { open } from "@tauri-apps/plugin-dialog";

export async function pickFolder(title: string, defaultPath?: string): Promise<string | null> {
  const picked = await open({ directory: true, multiple: false, title, defaultPath });
  return typeof picked === "string" ? picked : null;
}

export async function pickFile(
  title: string,
  extensions: string[],
  defaultPath?: string,
): Promise<string | null> {
  const picked = await open({
    directory: false,
    multiple: false,
    title,
    defaultPath,
    filters: [{ name: title, extensions }],
  });
  return typeof picked === "string" ? picked : null;
}
