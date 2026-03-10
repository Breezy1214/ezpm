import { strict as assert } from "node:assert";
import { COMMAND_SPECS } from "../src/commands/commandSpecs";

describe("command parity", () => {
	it("keeps expected CLI subcommand mapping", () => {
		const byId = new Map(
			COMMAND_SPECS.map((spec) => [spec.commandId, spec.args]),
		);
		assert.deepEqual(byId.get("ezpm.fixRequires"), ["fix-requires"]);
		assert.deepEqual(byId.get("ezpm.setupWallyPackages"), [
			"setup-wally-packages",
		]);
		assert.deepEqual(byId.get("ezpm.formatCheck"), ["format", "--check"]);
	});
});
