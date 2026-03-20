export interface CommandSpec {
	commandId: string;
	args: string[];
	execution: "runner" | "terminal";
	successMessage?: string;
}

export const COMMAND_SPECS: CommandSpec[] = [
	{
		commandId: "ezpm.install",
		args: ["install"],
		execution: "runner",
		successMessage: "ezpm install completed.",
	},
	{
		commandId: "ezpm.lint",
		args: ["lint"],
		execution: "runner",
		successMessage: "ezpm lint completed.",
	},
	{
		commandId: "ezpm.format",
		args: ["format"],
		execution: "runner",
		successMessage: "ezpm format completed.",
	},
	{
		commandId: "ezpm.formatCheck",
		args: ["format", "--check"],
		execution: "runner",
		successMessage: "ezpm format --check passed.",
	},
	{
		commandId: "ezpm.docs",
		args: ["docs"],
		execution: "terminal",
	},
	{
		commandId: "ezpm.fixRequires",
		args: ["fix-requires"],
		execution: "runner",
		successMessage: "ezpm fix-requires completed.",
	},
	{
		commandId: "ezpm.check",
		args: ["check"],
		execution: "runner",
		successMessage: "ezpm check passed.",
	},
	{
		commandId: "ezpm.setupWallyPackages",
		args: ["setup-wally-packages"],
		execution: "runner",
		successMessage: "ezpm setup-wally-packages completed.",
	},
	{
		commandId: "ezpm.init",
		args: ["init"],
		execution: "terminal",
	},
	{
		commandId: "ezpm.alias",
		args: ["alias"],
		execution: "terminal",
	},
];
