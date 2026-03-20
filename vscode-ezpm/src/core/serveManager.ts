import { ChildProcess, spawn } from "node:child_process";
import * as vscode from "vscode";
import { resolveEzpmBinary, showMissingBinaryHelp } from "./binaryResolver";
import { buildServeArgs } from "./serveArgs";

interface ServeSession {
	process: ChildProcess;
	startedAt: number;
	port?: number;
	killTimer?: ReturnType<typeof setTimeout>;
}

export class ServeManager implements vscode.Disposable {
	private readonly sessions = new Map<string, ServeSession>();

	public constructor(private readonly output: vscode.OutputChannel) {}

	public async start(
		folder: vscode.WorkspaceFolder,
		configuredPort?: number,
	): Promise<void> {
		const key = folder.uri.fsPath;
		if (this.sessions.has(key)) {
			void vscode.window.showInformationMessage(
				`ezpm serve is already running in ${folder.name}.`,
			);
			return;
		}

		const binary = resolveEzpmBinary(folder.uri);
		const args = buildServeArgs(configuredPort);

		this.output.appendLine(
			`[serve:start] ${binary} ${args.join(" ")} (cwd=${key})`,
		);

		const child = spawn(binary, args, {
			cwd: key,
			env: process.env,
			stdio: ["ignore", "pipe", "pipe"],
		});

		child.stdout.on("data", (chunk: Buffer) =>
			this.output.append(chunk.toString()),
		);
		child.stderr.on("data", (chunk: Buffer) =>
			this.output.append(chunk.toString()),
		);

		child.on("error", async (error: NodeJS.ErrnoException) => {
			this.sessions.delete(key);
			if (error.code === "ENOENT") {
				await showMissingBinaryHelp(binary);
			}
			this.output.appendLine(`[serve:error] ${error.message}`);
			void vscode.window.showErrorMessage(
				`Failed to start ezpm serve: ${error.message}`,
			);
		});

		child.on("close", (code, signal) => {
			const session = this.sessions.get(key);
			if (session?.killTimer) {
				clearTimeout(session.killTimer);
			}
			this.sessions.delete(key);
			this.output.appendLine(
				`[serve:exit] code=${code ?? "null"} signal=${signal ?? "none"} cwd=${key}`,
			);
			void vscode.window.showInformationMessage(
				`ezpm serve stopped in ${folder.name}.`,
			);
		});

		this.sessions.set(key, {
			process: child,
			startedAt: Date.now(),
			port: configuredPort,
		});

		void vscode.window.showInformationMessage(
			`Started ezpm serve in ${folder.name}.`,
		);
	}

	public async stop(folder: vscode.WorkspaceFolder): Promise<void> {
		const key = folder.uri.fsPath;
		const session = this.sessions.get(key);
		if (!session) {
			void vscode.window.showInformationMessage(
				`ezpm serve is not running in ${folder.name}.`,
			);
			return;
		}

		this.output.appendLine(`[serve:stop] cwd=${key}`);
		const stopped = session.process.kill("SIGINT");
		if (!stopped) {
			this.output.appendLine(
				"[serve:stop] SIGINT was not delivered; forcing SIGKILL",
			);
			session.process.kill("SIGKILL");
			this.sessions.delete(key);
			return;
		}

		session.killTimer = setTimeout(() => {
			if (this.sessions.has(key)) {
				this.output.appendLine(
					"[serve:stop] timeout waiting for graceful shutdown; forcing SIGKILL",
				);
				session.process.kill("SIGKILL");
				this.sessions.delete(key);
			}
		}, 5000);
	}

	public status(folder: vscode.WorkspaceFolder): string {
		const key = folder.uri.fsPath;
		const session = this.sessions.get(key);
		if (!session) {
			return `ezpm serve is not running in ${folder.name}.`;
		}

		const elapsedSec = Math.floor((Date.now() - session.startedAt) / 1000);
		const portInfo = session.port ? ` on port ${session.port}` : "";
		return `ezpm serve is running in ${folder.name}${portInfo} (${elapsedSec}s).`;
	}

	public async stopAll(): Promise<void> {
		const folders = vscode.workspace.workspaceFolders ?? [];
		for (const folder of folders) {
			await this.stop(folder);
		}
	}

	public dispose(): void {
		for (const session of this.sessions.values()) {
			if (session.killTimer) {
				clearTimeout(session.killTimer);
			}
			session.process.kill("SIGKILL");
		}
		this.sessions.clear();
	}
}
