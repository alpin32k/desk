import { describe, expect, it } from "vitest";
import { detect, shortcuts } from "./platform";

describe("platform", () => {
  it("detects the OS from the user agent", () => {
    expect(detect("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)")).toBe("mac");
    expect(detect("Mozilla/5.0 (Windows NT 10.0; Win64; x64)")).toBe("windows");
    expect(detect("Mozilla/5.0 (X11; Linux x86_64)")).toBe("linux");
  });

  it("rewrites shortcuts inside text", () => {
    expect(shortcuts("Druga sesja (⌘D), zabij (⌘⇧Z)", "windows")).toBe("Druga sesja (Ctrl⇧D), zabij (Ctrl⇧⌫)");
    expect(shortcuts("Druga sesja (⌘D)", "mac")).toBe("Druga sesja (⌘D)");
  });
});
