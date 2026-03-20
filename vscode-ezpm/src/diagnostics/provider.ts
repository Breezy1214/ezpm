import * as vscode from "vscode";
import { EzpmRunner } from "../core/runner";
import { toDiagnostics } from "./checkJson";
import { parseCheckJson } from "./parser";

export class EzpmDiagnosticsProvider implements vscode.Disposable {
	private readonly collection: vscode.DiagnosticCollection;

	public constructor(
		private readonly runner: EzpmRunner,
		private readonly output: vscode.OutputChannel,
	) {
		this.collection = vscode.languages.createDiagnosticCollection("ezpm");
	}

	public async refresh(
		folder: vscode.WorkspaceFolder,
		options?: { silent?: boolean },
	): Promise<void> {
		const silent = options?.silent ?? false;

		let runResult;
		try {
			runResult = await this.runner.run({
				cwd: folder.uri.fsPath,
				args: ["check", "--json"],
				resource: folder.uri,
			});
		} catch (error) {
			this.output.appendLine(
				`[diagnostics] Failed to run ezpm check --json: ${String(error)}`,
			);
			if (!silent) {
				void vscode.window.showErrorMessage(
					"Failed to run `ezpm check --json`. See the 'ezpm' output channel for details.",
				);
			}
			return;
		}

		const raw = runResult.stdout.trim();
		if (!raw) {
			this.output.appendLine(
				"[diagnostics] No JSON output from ezpm check --json; clearing diagnostics.",
			);
			this.collection.clear();
			if (runResult.exitCode !== 0) {
				if (!silent) {
					void vscode.window.showWarningMessage(
						"ezpm check failed and did not return JSON diagnostics.",
					);
				}
			}
			return;
		}

		try {
			const parsed = parseCheckJson(raw);
			const byFile = toDiagnostics(folder, parsed);
			this.collection.clear();
			for (const [uri, diagnostics] of byFile.entries()) {
				this.collection.set(vscode.Uri.parse(uri), diagnostics);
			}

			if (silent) {
				return;
			}

			if (runResult.exitCode === 0) {
				void vscode.window.showInformationMessage(
					"ezpm diagnostics refreshed.",
				);
			} else {
				void vscode.window.showWarningMessage(
					"ezpm check reported dependency issues. Diagnostics updated.",
				);
			}
		} catch (error) {
			this.output.appendLine(
				`[diagnostics] Failed to parse check JSON: ${String(error)}`,
			);
			this.collection.clear();
			if (!silent) {
				void vscode.window.showErrorMessage(
					"Failed to parse output from `ezpm check --json`. Diagnostics cleared.",
				);
			}
		}
	}

	public clear(): void {
		this.collection.clear();
	}

	public dispose(): void {
		this.collection.dispose();
	}
}
