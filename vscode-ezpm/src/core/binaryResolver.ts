import * as path from "node:path";
import * as vscode from "vscode";

export function resolveEzpmBinary(resource?: vscode.Uri): string {
	const configuredPath = vscode.workspace
		.getConfiguration("ezpm", resource)
		.get<string>("binaryPath", "")
		.trim();
	if (!configuredPath) {
		return "ezpm";
	}

	return path.resolve(configuredPath);
}

export async function showMissingBinaryHelp(binaryPath: string): Promise<void> {
	const openSettings = "Open Settings";
	const installHint = "View Install Guide";
	const selected = await vscode.window.showErrorMessage(
		`Failed to run '${binaryPath}'. Install ezpm or configure 'ezpm.binaryPath'.`,
		openSettings,
		installHint,
	);

	if (selected === openSettings) {
		await vscode.commands.executeCommand(
			"workbench.action.openSettings",
			"ezpm.binaryPath",
		);
		return;
	}

	if (selected === installHint) {
		await vscode.env.openExternal(
			vscode.Uri.parse("https://github.com/Breezy1214/ezpm"),
		);
	}
}
