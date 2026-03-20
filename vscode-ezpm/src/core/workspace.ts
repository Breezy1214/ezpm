import * as vscode from "vscode";

export async function pickWorkspaceFolder(): Promise<
	vscode.WorkspaceFolder | undefined
> {
	const folders = vscode.workspace.workspaceFolders;
	if (!folders || folders.length === 0) {
		void vscode.window.showErrorMessage(
			"Open a workspace folder before running ezpm commands.",
		);
		return undefined;
	}

	if (folders.length === 1) {
		return folders[0];
	}

	const selection = await vscode.window.showWorkspaceFolderPick({
		placeHolder: "Select workspace folder for ezpm command",
	});
	return selection;
}
