import { describe, expect, it } from "vitest";
import { encodeToolConfirmAllow } from "../components/chat/toolConfirmAnswer";

describe("encodeToolConfirmAllow", () => {
  it("returns allow when whitelist is not requested", () => {
    expect(encodeToolConfirmAllow(false)).toBe("allow");
  });

  it("returns allow:whitelist when whitelist is requested", () => {
    expect(encodeToolConfirmAllow(true)).toBe("allow:whitelist");
  });
});
