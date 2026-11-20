import { strict as assert } from "node:assert";
import { parseCheckJson } from "../src/diagnostics/parser";

describe("checkJson parser", () => {
	it("parses expected fields", () => {
		const parsed = parseCheckJson(
			JSON.stringify({
				cycles: [{ modules: ["src/a.luau", "src/b.luau"] }],
				rule_violations: [
					{
						from_module: "src/client/a.luau",
						to_module: "src/server/b.luau",
						from_layer: "client",
						to_layer: "server",
					},
				],
				unused_modules: ["src/shared/c.luau"],
			}),
		);

		assert.equal(parsed.cycles.length, 1);
		assert.equal(parsed.rule_violations.length, 1);
		assert.equal(parsed.unused_modules.length, 1);
	});

	it("defaults absent arrays", () => {
		const parsed = parseCheckJson("{}");
		assert.deepEqual(parsed.cycles, []);
		assert.deepEqual(parsed.rule_violations, []);
		assert.deepEqual(parsed.feature_violations, []);
		assert.deepEqual(parsed.unused_modules, []);
	});

	it("keeps require lines and feature violations", () => {
		const parsed = parseCheckJson(
			JSON.stringify({
				rule_violations: [
					{
						from_module: "src/client/a.luau",
						to_module: "src/server/b.luau",
						from_layer: "client",
						to_layer: "server",
						line: 4,
					},
				],
				feature_violations: [
					{
						from_module: "src/server/Shop/init.luau",
						to_module: "src/shared/Inventory/init.luau",
						from_feature: "Shop",
						to_feature: "Inventory",
						line: 2,
					},
				],
			}),
		);

		assert.equal(parsed.rule_violations[0].line, 4);
		assert.equal(parsed.feature_violations[0].to_feature, "Inventory");
	});
});
