export interface CheckCycle {
	modules: string[];
}

export interface CheckRuleViolation {
	from_module: string;
	to_module: string;
	from_layer: string;
	to_layer: string;
	reason?: string;
}

export interface CheckResult {
	cycles: CheckCycle[];
	rule_violations: CheckRuleViolation[];
	unused_modules: string[];
}

export function parseCheckJson(raw: string): CheckResult {
	const parsed = JSON.parse(raw) as Partial<CheckResult>;
	return {
		cycles: parsed.cycles ?? [],
		rule_violations: parsed.rule_violations ?? [],
		unused_modules: parsed.unused_modules ?? [],
	};
}
