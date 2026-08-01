import * as vscode from "vscode";
import cp from "child_process";
import { log } from "./logger";
import { Context } from "./state";

export class Synchronizer {
  private readonly host: string;
  private readonly user: string;

  constructor(host: string) {
    [this.user, this.host] = host.split("@");
  }

  Synchronize(guestPath: string, hostPath: string): Promise<void> {
    const rclonePath = vscode.Uri.joinPath(
      Context.extensionURI,
      "bin",
      "rclone",
      "rclone.exe",
    );

    return new Promise<void>((resolve, reject) => {
      const rclone = cp.spawn(rclonePath.fsPath, [
        "sync",
        "--contimeout",
        "5s",
        "-q",
        guestPath,
        `:sftp,host=${this.host},user=${this.user}:${hostPath}`,
      ]);

      rclone.stdout.on("data", (data: Buffer) => {
        log(data.toString());
      });

      rclone.stderr.on("data", (data: Buffer) => {
        log(data.toString(), "ERROR");
        reject("Failed to synchronize folders");
      });

      rclone.on("close", () => {
        resolve();
      });
    });
  }
}
