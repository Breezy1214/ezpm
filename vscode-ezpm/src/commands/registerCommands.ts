import * as vscode from "vscode";
import { resolveEzpmBinary } from "../core/binaryResolver";
import { EzpmRunner } from "../core/runner";
import { ServeManager } from "../core/serveManager";
import { pickWorkspaceFolder } from "../core/workspace";
import { EzpmDiagnosticsProvider } from "../diagnostics/provider";
import { COMMAND_SPECS } from "./commandSpecs";

async function runCommandAndReport(
	runner: EzpmRunner,
	commandArgs: string[],
	successMessage: string,
): Promise<void> {
	const folder = await pickWorkspaceFolder();
	if (!folder) {
		return;
	}

	const result = await runner.run({
		cwd: folder.uri.fsPath,
		args: commandArgs,
	});

	if (result.exitCode === 0) {
		void vscode.window.showInformationMessage(successMessage);
		return;
	}

	void vscode.window.showErrorMessage(
		`ezpm ${commandArgs.join(" ")} failed with exit code ${result.exitCode ?? "null"}. See 'ezpm' output for details.`,
	);
}

async function runInteractiveInTerminal(commandArgs: string[]): Promise<void> {
	const folder = await pickWorkspaceFolder();
	if (!folder) {
		return;
	}

	const binary = resolveEzpmBinary();
	const terminal = vscode.window.createTerminal({
		name: `ezpm ${commandArgs[0]}`,
		cwd: folder.uri.fsPath,
	});
	terminal.show(true);
	terminal.sendText(`${binary} ${commandArgs.join(" ")}`, true);
}

export function registerCommands(
	context: vscode.ExtensionContext,
	runner: EzpmRunner,
	serveManager: ServeManager,
	diagnosticsProvider: EzpmDiagnosticsProvider,
): void {
	const register = (
		command: string,
		callback: (...args: unknown[]) => unknown,
	): void => {
		context.subscriptions.push(
			vscode.commands.registerCommand(command, callback),
		);
	};

	for (const spec of COMMAND_SPECS) {
		if (spec.interactive) {
			register(spec.commandId, async () => {
				await runInteractiveInTerminal(spec.args);
			});
			continue;
		}

		register(spec.commandId, async () => {
			await runCommandAndReport(runner, spec.args, spec.successMessage);
		});
	}

	register("ezpm.diagnostics.refresh", async () => {
		const folder = await pickWorkspaceFolder();
		if (!folder) {
			return;
		}
		await diagnosticsProvider.refresh(folder);
	});

	register("ezpm.serve.start", async () => {
		const folder = await pickWorkspaceFolder();
		if (!folder) {
			return;
		}

		const port = vscode.workspace
			.getConfiguration("ezpm", folder.uri)
			.get<number>("servePort", 0);
		await serveManager.start(folder, port);
	});

	register("ezpm.serve.stop", async () => {
		const folder = await pickWorkspaceFolder();
		if (!folder) {
			return;
		}

		await serveManager.stop(folder);
	});

	register("ezpm.serve.status", async () => {
		const folder = await pickWorkspaceFolder();
		if (!folder) {
			return;
		}

		const status = serveManager.status(folder);
		void vscode.window.showInformationMessage(status);
	});
}
