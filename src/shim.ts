import * as vscode from "vscode";
import { Context } from "./state";
import { lstat, rename, rm, symlink } from "fs/promises";
import { existsSync } from "fs";

export async function Setup() {
  const vsdbgBinURI = vscode.Uri.joinPath(
    Context.cpptoolsURI,
    "debugAdapters",
    "vsdbg",
    "bin",
  );

  const stats = await lstat(vsdbgBinURI.fsPath);

  if (stats.isSymbolicLink()) {
    // The symlink already exists, do nothing.
    return;
  }

  const vsdbgBinTempURI = vscode.Uri.joinPath(
    Context.cpptoolsURI,
    "debugAdapters",
    "vsdbg",
    "bin_temp",
  );

  await rename(vsdbgBinURI.fsPath, vsdbgBinTempURI.fsPath);

  const shimURI = vscode.Uri.joinPath(
    Context.extensionURI,
    "bin",
    "shim",
    "bin",
  );

  // The junction type does not require elevation, as opposed to the dir type
  await symlink(shimURI.fsPath, vsdbgBinURI.fsPath, "junction");
}

export async function Undo() {
  const vsdbgBinURI = vscode.Uri.joinPath(
    Context.cpptoolsURI,
    "debugAdapters",
    "vsdbg",
    "bin",
  );

  const exists = existsSync(vsdbgBinURI.fsPath);

  if (exists) {
    const stats = await lstat(vsdbgBinURI.fsPath);
    if (!stats.isSymbolicLink()) {
      // There's no symlink to undo.
      return;
    }
    await rm(vsdbgBinURI.fsPath);
  }

  const vsdbgBinTempURI = vscode.Uri.joinPath(
    Context.cpptoolsURI,
    "debugAdapters",
    "vsdbg",
    "bin_temp",
  );

  await rename(vsdbgBinTempURI.fsPath, vsdbgBinURI.fsPath);
}
