export interface CommandSpec {
	commandId: string;
	args: string[];
	successMessage: string;
	interactive: boolean;
}

export const COMMAND_SPECS: CommandSpec[] = [
	{
		commandId: "ezpm.install",
		args: ["install"],
		successMessage: "ezpm install completed.",
		interactive: false,
	},
	{
		commandId: "ezpm.lint",
		args: ["lint"],
		successMessage: "ezpm lint completed.",
		interactive: false,
	},
	{
		commandId: "ezpm.format",
		args: ["format"],
		successMessage: "ezpm format completed.",
		interactive: false,
	},
	{
		commandId: "ezpm.formatCheck",
		args: ["format", "--check"],
		successMessage: "ezpm format --check passed.",
		interactive: false,
	},
	{
		commandId: "ezpm.docs",
		args: ["docs"],
		successMessage: "ezpm docs finished.",
		interactive: false,
	},
	{
		commandId: "ezpm.fixRequires",
		args: ["fix-requires"],
		successMessage: "ezpm fix-requires completed.",
		interactive: false,
	},
	{
		commandId: "ezpm.check",
		args: ["check"],
		successMessage: "ezpm check passed.",
		interactive: false,
	},
	{
		commandId: "ezpm.setupWallyPackages",
		args: ["setup-wally-packages"],
		successMessage: "ezpm setup-wally-packages completed.",
		interactive: false,
	},
	{
		commandId: "ezpm.init",
		args: ["init"],
		successMessage: "",
		interactive: true,
	},
	{
		commandId: "ezpm.alias",
		args: ["alias"],
		successMessage: "",
		interactive: true,
	},
];
