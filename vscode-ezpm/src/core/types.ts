import * as vscode from "vscode";

export interface EzpmRunResult {
	exitCode: number | null;
	signal: NodeJS.Signals | null;
	stdout: string;
	stderr: string;
	durationMs: number;
}

export interface EzpmRunOptions {
	cwd: string;
	args: string[];
	resource?: vscode.Uri;
}
