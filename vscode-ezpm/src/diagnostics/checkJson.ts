import * as path from "node:path";
import * as vscode from "vscode";
import { CheckResult } from "./parser";

function resolvePath(
	workspaceFolder: vscode.WorkspaceFolder,
	modulePath: string,
): vscode.Uri {
	if (path.isAbsolute(modulePath)) {
		return vscode.Uri.file(modulePath);
	}
	return vscode.Uri.file(path.join(workspaceFolder.uri.fsPath, modulePath));
}

/** The require's line when ezpm reports one, else the start of the file. */
function lineRange(line?: number): vscode.Range {
	const index = line && line > 0 ? line - 1 : 0;
	return new vscode.Range(
		new vscode.Position(index, 0),
		new vscode.Position(index, line ? Number.MAX_SAFE_INTEGER : 1),
	);
}

export function toDiagnostics(
	workspaceFolder: vscode.WorkspaceFolder,
	result: CheckResult,
): Map<string, vscode.Diagnostic[]> {
	const byFile = new Map<string, vscode.Diagnostic[]>();

	const addDiagnostic = (
		fileUri: vscode.Uri,
		diagnostic: vscode.Diagnostic,
	): void => {
		const key = fileUri.toString();
		const current = byFile.get(key) ?? [];
		current.push(diagnostic);
		byFile.set(key, current);
	};

	for (const cycle of result.cycles) {
		const cycleText =
			cycle.modules.length > 0
				? `${cycle.modules.join(" -> ")} -> ${cycle.modules[0]}`
				: "(empty cycle)";
		for (const modulePath of cycle.modules) {
			const fileUri = resolvePath(workspaceFolder, modulePath);
			const diagnostic = new vscode.Diagnostic(
				lineRange(),
				`Circular dependency detected: ${cycleText}`,
				vscode.DiagnosticSeverity.Error,
			);
			diagnostic.source = "ezpm check";
			addDiagnostic(fileUri, diagnostic);
		}
	}

	for (const violation of result.rule_violations) {
		const fileUri = resolvePath(workspaceFolder, violation.from_module);
		const reason = violation.reason ? ` Reason: ${violation.reason}` : "";
		const diagnostic = new vscode.Diagnostic(
			lineRange(violation.line),
			`Architecture rule violation: ${violation.from_layer} -> ${violation.to_layer} is forbidden (${violation.from_module} -> ${violation.to_module}).${reason}`,
			vscode.DiagnosticSeverity.Error,
		);
		diagnostic.source = "ezpm check";
		addDiagnostic(fileUri, diagnostic);
	}

	for (const violation of result.feature_violations) {
		const fileUri = resolvePath(workspaceFolder, violation.from_module);
		const diagnostic = new vscode.Diagnostic(
			lineRange(violation.line),
			`Feature boundary violation: feature ${violation.from_feature} may not require feature ${violation.to_feature} (${violation.to_module}).`,
			vscode.DiagnosticSeverity.Error,
		);
		diagnostic.source = "ezpm check";
		addDiagnostic(fileUri, diagnostic);
	}

	for (const modulePath of result.unused_modules) {
		const fileUri = resolvePath(workspaceFolder, modulePath);
		const diagnostic = new vscode.Diagnostic(
			lineRange(),
			`Unused module: ${modulePath}`,
			vscode.DiagnosticSeverity.Warning,
		);
		diagnostic.source = "ezpm check";
		addDiagnostic(fileUri, diagnostic);
	}

	return byFile;
}
