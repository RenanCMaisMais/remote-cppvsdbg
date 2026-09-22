import * as vscode from "vscode";
import { Synchronizer } from "./synchronizer";
import * as path from "path";
import * as os from "os";
import { Context } from "./state";
import * as shim from "./shim";
import { readdir, readFile } from "fs/promises";
import { outputChannelName } from "./logger";

interface DebugConfiguration extends vscode.DebugConfiguration {
  remote: {
    host: string;
    synchronize: Object;
    interactive: boolean;
  };
}

export class DebugConfigurationProvider
  implements vscode.DebugConfigurationProvider
{
  async resolveDebugConfigurationWithSubstitutedVariables(
    folder: vscode.WorkspaceFolder | undefined,
    config: DebugConfiguration,
    token?: vscode.CancellationToken,
  ): Promise<vscode.DebugConfiguration | null | undefined> {
    if (!config.remote) {
      return config;
    }

    if (!config.remote.host || config.remote.host.length === 0) {
      void vscode.window.showErrorMessage("You must specify a host to debug!");
      return null;
    }

    // The symlink might not have been undone due to improper termination of vscode
    await shim.Undo();
    try {
      await this.SynchronizeFolders(config.remote);
    } catch (e) {
      vscode.window.showErrorMessage(
        `Failed to synchronize with remote, check the ${outputChannelName} output panel for detailed error information`,
      );
      return;
    }

    // Allows retriaval of the SSH host by the shim process.
    process.env["VSDBG_SSH_HOST"] = config.remote.host;
    process.env["VSDBG_INTERACTIVE"] = config.remote.interactive ? "TRUE" : "FALSE";

    await shim.Setup();

    return config;
  }

  private async SynchronizeFolders(config: DebugConfiguration["remote"]) {
    return vscode.window.withProgress(
      {
        location: vscode.ProgressLocation.Notification,
        title: `Syncing with remote target...`,
        cancellable: false,
      },
      async (progress, token) => {
        const synchronizer = new Synchronizer(config.host);
        const vsdbgPath = vscode.Uri.joinPath(
          Context.cpptoolsURI,
          "debugAdapters",
          "vsdbg",
          "bin",
        );

        const executablesPath = vscode.Uri.joinPath(
          Context.extensionURI,
          "bin",
          "shim",
          "bin",
        );

        let awaits: Thenable<void>[] = [];
        awaits.push(
          synchronizer.Synchronize(vsdbgPath.fsPath, ".cppvsdbg/bin"),
        );

        awaits.push(
          synchronizer.Synchronize(executablesPath.fsPath, ".cppvsdbg/spawner", {
            files: ["spawner.exe"],
          }),
        );

        const extensionsDir = path.join(".cppvsdbg", "extensions");
        for (const { filename, targetPath } of await this.getExtensionLinks()) {
          let extensionRemotePath = path.join(extensionsDir, filename);
          awaits.push(
            synchronizer.Synchronize(targetPath, extensionRemotePath),
          );
        }

        for (const [key, value] of Object.entries(config.synchronize)) {
          awaits.push(synchronizer.Synchronize(key, value));
        }

        return Promise.all(awaits);
      },
    );
  }

  private async getExtensionLinks(): Promise<
    {
      filename: string;
      targetPath: string;
    }[]
  > {
    const extensionsDir = path.resolve(os.homedir(), ".cppvsdbg", "extensions");

    const entries = await readdir(extensionsDir, { withFileTypes: true });

    const linkFiles = entries.filter(
      (entry) => entry.isFile() && entry.name.toLowerCase().endsWith(".link"),
    );

    const results = await Promise.all(
      linkFiles.map(async (file) => {
        const filePath = path.join(extensionsDir, file.name);
        const content = await readFile(filePath, "utf8");

        return {
          filename: file.name.replace(".link", ""),
          targetPath: content.trim(),
        };
      }),
    );

    return results;
  }
}
