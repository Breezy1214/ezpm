import * as vscode from "vscode";
import { resolveEzpmBinary } from "../core/binaryResolver";
import { EzpmRunner } from "../core/runner";
import { quoteForShell } from "../core/shellQuote";
import { ServeManager } from "../core/serveManager";
import { pickWorkspaceFolder } from "../core/workspace";
import { EzpmDiagnosticsProvider } from "../diagnostics/provider";
import { COMMAND_SPECS } from "./commandSpecs";

async function runCommandAndReport(
	runner: EzpmRunner,
	folder: vscode.WorkspaceFolder,
	commandArgs: string[],
	successMessage?: string,
): Promise<void> {
	try {
		const result = await runner.run({
			cwd: folder.uri.fsPath,
			args: commandArgs,
			resource: folder.uri,
		});

		if (result.exitCode === 0) {
			if (successMessage) {
				void vscode.window.showInformationMessage(successMessage);
			}
			return;
		}

		void vscode.window.showErrorMessage(
			`ezpm ${commandArgs.join(" ")} failed with exit code ${result.exitCode ?? "null"}. See 'ezpm' output for details.`,
		);
	} catch (error) {
		void vscode.window.showErrorMessage(
			`Failed to run ezpm ${commandArgs.join(" ")}: ${error instanceof Error ? error.message : String(error)}`,
		);
	}
}

function runInTerminal(
	folder: vscode.WorkspaceFolder,
	commandArgs: string[],
): void {
	const binary = resolveEzpmBinary(folder.uri);
	const terminal = vscode.window.createTerminal({
		name: `ezpm ${commandArgs[0]}`,
		cwd: folder.uri.fsPath,
	});
	terminal.show(true);
	terminal.sendText(
		[
			quoteForShell(binary),
			...commandArgs.map((arg) => quoteForShell(arg)),
		].join(" "),
		true,
	);
}

export function registerCommands(
	context: vscode.ExtensionContext,
	runner: EzpmRunner,
	serveManager: ServeManager,
	diagnosticsProvider: EzpmDiagnosticsProvider,
): void {
	const withFolder = async (
		callback: (folder: vscode.WorkspaceFolder) => Promise<void> | void,
	): Promise<void> => {
		const folder = await pickWorkspaceFolder();
		if (!folder) {
			return;
		}

		await callback(folder);
	};

	const register = (
		command: string,
		callback: (...args: unknown[]) => unknown,
	): void => {
		context.subscriptions.push(
			vscode.commands.registerCommand(command, callback),
		);
	};

	for (const spec of COMMAND_SPECS) {
		if (spec.execution === "terminal") {
			register(spec.commandId, async () => {
				await withFolder((folder) => {
					runInTerminal(folder, spec.args);
				});
			});
			continue;
		}

		register(spec.commandId, async () => {
			await withFolder((folder) =>
				runCommandAndReport(runner, folder, spec.args, spec.successMessage),
			);
		});
	}

	register("ezpm.diagnostics.refresh", async () => {
		await withFolder((folder) => diagnosticsProvider.refresh(folder));
	});

	register("ezpm.serve.start", async () => {
		await withFolder(async (folder) => {
			const port = vscode.workspace
				.getConfiguration("ezpm", folder.uri)
				.get<number>("servePort", 0);
			await serveManager.start(folder, port);
		});
	});

	register("ezpm.serve.stop", async () => {
		await withFolder((folder) => serveManager.stop(folder));
	});

	register("ezpm.serve.status", async () => {
		await withFolder((folder) => {
			const status = serveManager.status(folder);
			void vscode.window.showInformationMessage(status);
		});
	});
}
