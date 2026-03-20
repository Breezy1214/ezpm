import { strict as assert } from "node:assert";
import { COMMAND_SPECS } from "../src/commands/commandSpecs";

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
});
