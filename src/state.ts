import * as vscode from "vscode";

export const Context = {
  extensionURI: null as unknown as vscode.Uri,
  cpptoolsURI: null as unknown as vscode.Uri,
};

export function Initialize(context: vscode.ExtensionContext) {
  Context.extensionURI = context.extensionUri;
  Context.cpptoolsURI =
    vscode.extensions.getExtension("ms-vscode.cpptools")!.extensionUri;
}
