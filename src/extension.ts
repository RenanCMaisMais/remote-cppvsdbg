import * as vscode from "vscode";
import { DebugConfigurationProvider } from "./configurationProvider";
import * as Extension from "./state";
import * as shim from "./shim";

export async function activate(context: vscode.ExtensionContext) {
  Extension.Initialize(context);

  // The symlink might not have been undone due to improper termination of vscode
  await shim.Undo();

  context.subscriptions.push(
    vscode.debug.registerDebugConfigurationProvider(
      "cppvsdbg",
      new DebugConfigurationProvider(),
    ),
  );

  // TODO: This does not differentiate between full terminations and session restarts, which causes sessions restarts to fail.
  vscode.debug.onDidTerminateDebugSession((session: vscode.DebugSession) => {
    // Only undo the symlink if the sessions is the root session and it's type is cppvsdbg
    if (session.type !== "cppvsdbg" || session.parentSession) {
      return;
    }

    shim.Undo();
  });
}

export async function deactivate() {
  await shim.Undo();
}
