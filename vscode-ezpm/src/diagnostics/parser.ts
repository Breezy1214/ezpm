export interface CheckCycle {
	modules: string[];
}

export interface CheckRuleViolation {
	from_module: string;
	to_module: string;
	from_layer: string;
	to_layer: string;
	reason?: string;
	/** 1-based line of the offending require (ezpm 0.4+). */
	line?: number;
}

export interface CheckFeatureViolation {
	from_module: string;
	to_module: string;
	from_feature: string;
	to_feature: string;
	line?: number;
}

export interface CheckResult {
	cycles: CheckCycle[];
	rule_violations: CheckRuleViolation[];
	feature_violations: CheckFeatureViolation[];
	unused_modules: string[];
}

export function parseCheckJson(raw: string): CheckResult {
	const parsed = JSON.parse(raw) as Partial<CheckResult>;
	return {
		cycles: parsed.cycles ?? [],
		rule_violations: parsed.rule_violations ?? [],
		feature_violations: parsed.feature_violations ?? [],
		unused_modules: parsed.unused_modules ?? [],
	};
}
