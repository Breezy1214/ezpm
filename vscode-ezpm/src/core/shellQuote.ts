export function quoteForShell(
	value: string,
	platform: NodeJS.Platform = process.platform,
): string {
	if (platform === "win32") {
		return `"${value.replace(/"/g, '\\"')}"`;
	}

	return `'${value.replace(/'/g, `'\\''`)}'`;
}
