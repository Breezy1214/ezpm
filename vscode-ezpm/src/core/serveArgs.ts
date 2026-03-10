export function buildServeArgs(configuredPort?: number): string[] {
	const args = ["serve"];
	if (configuredPort && configuredPort > 0) {
		args.push("-p", String(configuredPort));
	}
	return args;
}
