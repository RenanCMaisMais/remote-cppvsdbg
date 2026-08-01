import * as vscode from "vscode";

export const outputChannelName = "Remote C++ Debugger";

const remoteOutput = vscode.window.createOutputChannel("Remote C++ Debugger");

export function log(message: string, level: "INFO" | "ERROR" = "INFO") {
  const timestamp = new Date().toLocaleTimeString();
  remoteOutput.appendLine(`[${timestamp}] [${level}] ${message}`);
}
