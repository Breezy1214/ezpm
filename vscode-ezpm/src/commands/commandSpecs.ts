export interface CommandSpec {
	commandId: string;
	label: string;
	description: string;
	args: string[];
	execution: "runner" | "terminal";
	successMessage?: string;
}

export const COMMAND_SPECS: CommandSpec[] = [
	{
		commandId: "ezpm.install",
		label: "Install",
		description: "Run ezpm install",
		args: ["install"],
		execution: "runner",
		successMessage: "ezpm install completed.",
	},
	{
		commandId: "ezpm.lint",
		label: "Lint",
		description: "Run ezpm lint",
		args: ["lint"],
		execution: "runner",
		successMessage: "ezpm lint completed.",
	},
	{
		commandId: "ezpm.format",
		label: "Format",
		description: "Run ezpm format",
		args: ["format"],
		execution: "runner",
		successMessage: "ezpm format completed.",
	},
	{
		commandId: "ezpm.formatCheck",
		label: "Format (Check Only)",
		description: "Run ezpm format --check",
		args: ["format", "--check"],
		execution: "runner",
		successMessage: "ezpm format --check passed.",
	},
	{
		commandId: "ezpm.docs",
		label: "Docs",
		description: "Open ezpm docs (terminal)",
		args: ["docs"],
		execution: "terminal",
	},
	{
		commandId: "ezpm.fixRequires",
		label: "Fix Requires",
		description: "Run ezpm fix-requires",
		args: ["fix-requires"],
		execution: "runner",
		successMessage: "ezpm fix-requires completed.",
	},
	{
		commandId: "ezpm.check",
		label: "Check",
		description: "Run ezpm check",
		args: ["check"],
		execution: "runner",
		successMessage: "ezpm check passed.",
	},
	{
		commandId: "ezpm.setupWallyPackages",
		label: "Setup Wally Packages",
		description: "Run ezpm setup-wally-packages",
		args: ["setup-wally-packages"],
		execution: "runner",
		successMessage: "ezpm setup-wally-packages completed.",
	},
	{
		commandId: "ezpm.init",
		label: "Init",
		description: "Run ezpm init (terminal)",
		args: ["init"],
		execution: "terminal",
	},
	{
		commandId: "ezpm.alias",
		label: "Alias",
		description: "Run ezpm alias (terminal)",
		args: ["alias"],
		execution: "terminal",
	},
];

export interface QuickPickCommand {
	label: string;
	description: string;
	commandId: string;
}

export const QUICK_PICK_ITEMS: QuickPickCommand[] = [
	...COMMAND_SPECS.map((spec) => ({
		label: spec.label,
		description: spec.description,
		commandId: spec.commandId,
	})),
	{
		label: "Start Serve",
		description: "Start ezpm serve",
		commandId: "ezpm.serve.start",
	},
	{
		label: "Stop Serve",
		description: "Stop ezpm serve",
		commandId: "ezpm.serve.stop",
	},
	{
		label: "Serve Status",
		description: "Check serve status",
		commandId: "ezpm.serve.status",
	},
	{
		label: "Refresh Diagnostics",
		description: "Refresh ezpm diagnostics",
		commandId: "ezpm.diagnostics.refresh",
	},
];
