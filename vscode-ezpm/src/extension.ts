import * as vscode from "vscode";
import { registerCommands } from "./commands/registerCommands";
import { EzpmRunner } from "./core/runner";
import { ServeManager } from "./core/serveManager";
import { EzpmDiagnosticsProvider } from "./diagnostics/provider";

let serveManagerRef: ServeManager | undefined;

const DIAGNOSTICS_DEBOUNCE_MS = 1000;

export function activate(context: vscode.ExtensionContext): void {
	const output = vscode.window.createOutputChannel("ezpm", { log: true });
	context.subscriptions.push(output);
	output.appendLine("[activate] ezpm extension activated");

	const runner = new EzpmRunner(output);
	const serveManager = new ServeManager(output);
	const diagnosticsProvider = new EzpmDiagnosticsProvider(runner, output);

	serveManagerRef = serveManager;

	context.subscriptions.push(serveManager, diagnosticsProvider);

	registerCommands(context, runner, serveManager, diagnosticsProvider);

	const pendingTimers = new Map<string, ReturnType<typeof setTimeout>>();
	context.subscriptions.push(
		new vscode.Disposable(() => {
			for (const timer of pendingTimers.values()) {
				clearTimeout(timer);
			}
			pendingTimers.clear();
		}),
	);

	context.subscriptions.push(
		vscode.workspace.onDidSaveTextDocument((document) => {
			const autoRefreshEnabled = vscode.workspace
				.getConfiguration("ezpm", document.uri)
				.get<boolean>("autoRefreshDiagnostics", true);
			if (!autoRefreshEnabled) {
				return;
			}

			const folder = vscode.workspace.getWorkspaceFolder(document.uri);
			if (!folder) {
				return;
			}

			if (
				!document.fileName.endsWith(".lua") &&
				!document.fileName.endsWith(".luau")
			) {
				return;
			}

			const key = folder.uri.fsPath;
			const existing = pendingTimers.get(key);
			if (existing) {
				clearTimeout(existing);
			}
			pendingTimers.set(
				key,
				setTimeout(() => {
					pendingTimers.delete(key);
					void diagnosticsProvider.refresh(folder, { silent: true });
				}, DIAGNOSTICS_DEBOUNCE_MS),
			);
		}),
	);
}

export function deactivate(): void {
	if (serveManagerRef) {
		serveManagerRef.dispose();
		serveManagerRef = undefined;
	}
}
