import { strict as assert } from "node:assert";
import { COMMAND_SPECS, QUICK_PICK_ITEMS } from "../src/commands/commandSpecs";

describe("command parity", () => {
	it("keeps expected CLI subcommand mapping", () => {
		const byId = new Map(COMMAND_SPECS.map((spec) => [spec.commandId, spec]));
		assert.deepEqual(byId.get("ezpm.fixRequires")?.args, ["fix-requires"]);
		assert.deepEqual(byId.get("ezpm.setupWallyPackages")?.args, [
			"setup-wally-packages",
		]);
		assert.deepEqual(byId.get("ezpm.formatCheck")?.args, ["format", "--check"]);
		assert.equal(byId.get("ezpm.docs")?.execution, "terminal");
		assert.equal(byId.get("ezpm.init")?.execution, "terminal");
		assert.equal(byId.get("ezpm.alias")?.execution, "terminal");
	});

	it("every spec has a non-empty label and description", () => {
		for (const spec of COMMAND_SPECS) {
			assert.ok(spec.label, `${spec.commandId} is missing label`);
			assert.ok(spec.description, `${spec.commandId} is missing description`);
		}
	});

	it("QUICK_PICK_ITEMS covers all spec commands plus manual commands", () => {
		const pickIds = QUICK_PICK_ITEMS.map((item) => item.commandId);
		for (const spec of COMMAND_SPECS) {
			assert.ok(
				pickIds.includes(spec.commandId),
				`${spec.commandId} missing from QUICK_PICK_ITEMS`,
			);
		}
		const manualIds = [
			"ezpm.serve.start",
			"ezpm.serve.stop",
			"ezpm.serve.status",
			"ezpm.diagnostics.refresh",
		];
		for (const id of manualIds) {
			assert.ok(
				pickIds.includes(id),
				`${id} missing from QUICK_PICK_ITEMS`,
			);
		}
	});

	it("every QUICK_PICK_ITEMS entry has label and description", () => {
		for (const item of QUICK_PICK_ITEMS) {
			assert.ok(item.label, `${item.commandId} is missing label`);
			assert.ok(item.description, `${item.commandId} is missing description`);
		}
	});
});
