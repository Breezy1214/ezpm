import { spawn } from "node:child_process";
import * as vscode from "vscode";
import { resolveEzpmBinary, showMissingBinaryHelp } from "./binaryResolver";
import { EzpmRunOptions, EzpmRunResult } from "./types";

export class EzpmRunner {
	public constructor(private readonly output: vscode.OutputChannel) {}

	public async run(options: EzpmRunOptions): Promise<EzpmRunResult> {
		const binary = resolveEzpmBinary(options.resource);
		const startedAt = Date.now();

		this.output.appendLine(
			`[run] ${binary} ${options.args.join(" ")} (cwd=${options.cwd})`,
		);

		const result = await new Promise<EzpmRunResult>((resolve, reject) => {
			const child = spawn(binary, options.args, {
				cwd: options.cwd,
				env: process.env,
				stdio: ["ignore", "pipe", "pipe"],
			});

			let stdout = "";
			let stderr = "";

			child.stdout.on("data", (chunk: Buffer) => {
				const text = chunk.toString();
				stdout += text;
				this.output.append(text);
			});

			child.stderr.on("data", (chunk: Buffer) => {
				const text = chunk.toString();
				stderr += text;
				this.output.append(text);
			});

			child.on("error", async (error: NodeJS.ErrnoException) => {
				if (error.code === "ENOENT") {
					await showMissingBinaryHelp(binary);
				}
				reject(error);
			});

			child.on("close", (exitCode, signal) => {
				resolve({
					exitCode,
					signal,
					stdout,
					stderr,
					durationMs: Date.now() - startedAt,
				});
			});
		});

		this.output.appendLine(
			`[done] exit=${result.exitCode ?? "null"} signal=${result.signal ?? "none"} durationMs=${result.durationMs}`,
		);

		return result;
	}
}
