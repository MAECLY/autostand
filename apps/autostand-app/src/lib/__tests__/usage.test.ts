/**
 * The contract under test: read what the provider reported, and nothing else.
 *
 * Every case here pins one rule the badge, the pre-flight and the Settings rail
 * all depend on — a missing value never becomes a zero, the threshold is always
 * the caller's, and a projection is never invented locally.
 */

import { describe, expect, it } from "vitest";

import {
  activeProvider,
  fallbackProvider,
  formatRunOut,
  healthFor,
  periodLabel,
  remainingPercent,
  tightestWindow,
  usagePressure,
  windowDescription,
  windowLabel,
} from "@/lib/usage";
import {
  makeAppConfig,
  makeProviderConfig,
  makeProviderHealth,
  makeUsageWindow,
} from "@/test/mocks";

const FIVE_HOURS_MS = 5 * 60 * 60 * 1000;

describe("remainingPercent", () => {
  it("prefers the reported remainder, then the reported usage", () => {
    expect(
      remainingPercent(makeUsageWindow({ remaining_percent: 82, used_percent: 18 })),
    ).toBe(82);
    expect(remainingPercent(makeUsageWindow({ used_percent: 18 }))).toBe(82);
  });

  it("derives the share from raw scalars when no percentage was sent", () => {
    expect(
      remainingPercent(makeUsageWindow({ used: 250, limit: 1000, unit: "requests" })),
    ).toBe(75);
  });

  it("reports nothing for a window that reported nothing", () => {
    expect(remainingPercent(makeUsageWindow())).toBeNull();
  });

  it("gives a balance no share, because a countdown has no denominator", () => {
    expect(
      remainingPercent(
        makeUsageWindow({ kind: "balance", available: 821, unit: "credits" }),
      ),
    ).toBeNull();
  });

  it("clamps a provider percentage that leaves the 0–100 range", () => {
    expect(remainingPercent(makeUsageWindow({ remaining_percent: 140 }))).toBe(100);
    expect(remainingPercent(makeUsageWindow({ used_percent: 140 }))).toBe(0);
  });
});

describe("tightestWindow", () => {
  it("picks the window closest to running out", () => {
    const health = makeProviderHealth({
      windows: [
        makeUsageWindow({ id: "session", remaining_percent: 40 }),
        makeUsageWindow({ id: "weekly", remaining_percent: 12 }),
        makeUsageWindow({ id: "sonnet", remaining_percent: 88 }),
      ],
    });
    expect(tightestWindow(health)?.window.id).toBe("weekly");
    expect(tightestWindow(health)?.remaining).toBe(12);
  });

  /** A window without data is passed over, never counted as empty. */
  it("ignores windows that reported no share", () => {
    const health = makeProviderHealth({
      windows: [
        makeUsageWindow({ id: "weekly" }),
        makeUsageWindow({ id: "session", remaining_percent: 55 }),
      ],
    });
    expect(tightestWindow(health)?.window.id).toBe("session");
  });

  it("is null when nothing reports a share at all", () => {
    expect(tightestWindow(makeProviderHealth({ windows: [makeUsageWindow()] }))).toBeNull();
    expect(tightestWindow(undefined)).toBeNull();
  });
});

describe("labels", () => {
  it("names a window by its period when the provider supplied one", () => {
    expect(periodLabel(FIVE_HOURS_MS)).toBe("5 h window");
    expect(periodLabel(30 * 60 * 1000)).toBe("30 min window");
    expect(periodLabel(7 * 24 * 60 * 60 * 1000)).toBe("7 d window");
    expect(periodLabel(null)).toBeNull();
    expect(periodLabel(0)).toBeNull();
  });

  it("prefers the provider's own label over the derived one", () => {
    expect(windowLabel(makeUsageWindow({ id: "five_hour" }))).toBe("Five Hour");
    expect(windowLabel(makeUsageWindow({ label: "Opus weekly" }))).toBe("Opus weekly");
  });

  it("describes a window by duration first, because a duration is what makes a percentage mean something", () => {
    expect(
      windowDescription(
        makeUsageWindow({ id: "five_hour", period_duration_ms: FIVE_HOURS_MS }),
      ),
    ).toBe("5 h window");
    expect(windowDescription(makeUsageWindow({ id: "five_hour" }))).toBe("Five Hour");
  });
});

describe("formatRunOut", () => {
  it("rounds coarsely, because a burn-rate projection is not a clock", () => {
    expect(formatRunOut(2_100)).toBe("~35 min");
    expect(formatRunOut(10)).toBe("~1 min");
    expect(formatRunOut(3 * 3_600)).toBe("~3 h");
    expect(formatRunOut(4 * 24 * 3_600)).toBe("~4 d");
  });

  /** The backend declined to project; the sentence is dropped, not guessed. */
  it("says nothing when there is no projection", () => {
    expect(formatRunOut(null)).toBeNull();
    expect(formatRunOut(undefined)).toBeNull();
    expect(formatRunOut(0)).toBeNull();
    expect(formatRunOut(Number.NaN)).toBeNull();
  });
});

describe("activeProvider", () => {
  it("follows the render chain: an explicit order wins over the legacy field", () => {
    const config = makeAppConfig();
    config.llm.preferred_provider = "claude";
    config.llm.provider_order = ["openai", "claude"];
    expect(activeProvider(config)).toBe("openai");
  });

  it("falls back to the preferred provider when no order is configured", () => {
    const config = makeAppConfig();
    config.llm.preferred_provider = "grok";
    config.llm.provider_order = [];
    expect(activeProvider(config)).toBe("grok");
  });

  it("is null when nothing is configured", () => {
    const config = makeAppConfig();
    config.llm.preferred_provider = "";
    config.llm.provider_order = ["  "];
    expect(activeProvider(config)).toBeNull();
    expect(activeProvider(undefined)).toBeNull();
  });
});

describe("usagePressure", () => {
  const lowWindow = makeUsageWindow({
    id: "five_hour",
    remaining_percent: 12,
    period_duration_ms: FIVE_HOURS_MS,
    runs_out_in_seconds: 2_100,
  });

  it("states the fact the pre-flight needs, from the configured threshold", () => {
    const pressure = usagePressure(
      makeProviderHealth({ windows: [lowWindow] }),
      20,
    );
    expect(pressure).toEqual({
      provider: "claude",
      window: lowWindow,
      remainingPercent: 12,
      windowDescription: "5 h window",
      runsOutIn: "~35 min",
    });
  });

  /** The threshold is the caller's; nothing here hardcodes 20. */
  it("stays silent above the caller's threshold and speaks below it", () => {
    const health = makeProviderHealth({
      windows: [makeUsageWindow({ remaining_percent: 30 })],
    });
    expect(usagePressure(health, 20)).toBeNull();
    expect(usagePressure(health, 35)?.remainingPercent).toBe(30);
  });

  it("omits the projection instead of inventing one", () => {
    const pressure = usagePressure(
      makeProviderHealth({
        windows: [makeUsageWindow({ remaining_percent: 5, period_duration_ms: FIVE_HOURS_MS })],
      }),
      20,
    );
    expect(pressure?.runsOutIn).toBeNull();
  });

  it("says nothing about a provider nobody measured", () => {
    expect(usagePressure(undefined, 20)).toBeNull();
    expect(usagePressure(makeProviderHealth({ windows: [makeUsageWindow()] }), 20)).toBeNull();
  });
});

describe("fallbackProvider", () => {
  function configWithOrder(order: string[]) {
    const config = makeAppConfig();
    config.llm.provider_order = order;
    config.llm.fallback_enabled = true;
    return config;
  }

  it("offers the next provider in the user's own order", () => {
    const config = configWithOrder(["claude", "openai", "grok"]);
    expect(fallbackProvider(config, [], 20)).toBe("openai");
  });

  /** Unknown is not bad news, it is no news — a fair suggestion. */
  it("offers a provider that has never been measured", () => {
    const config = configWithOrder(["claude", "openai"]);
    const health = [makeProviderHealth({ provider: "claude", availability: "low" })];
    expect(fallbackProvider(config, health, 20)).toBe("openai");
  });

  it("passes over a provider the render chain would skip anyway", () => {
    const config = configWithOrder(["claude", "openai", "grok"]);
    const health = [
      makeProviderHealth({ provider: "openai", availability: "auth_required" }),
    ];
    expect(fallbackProvider(config, health, 20)).toBe("grok");
  });

  it("passes over a provider that is itself under pressure", () => {
    const config = configWithOrder(["claude", "openai", "grok"]);
    const health = [
      makeProviderHealth({
        provider: "openai",
        windows: [makeUsageWindow({ remaining_percent: 4 })],
      }),
    ];
    expect(fallbackProvider(config, health, 20)).toBe("grok");
  });

  it("offers nothing when fallback is switched off", () => {
    const config = configWithOrder(["claude", "openai"]);
    config.llm.fallback_enabled = false;
    expect(fallbackProvider(config, [], 20)).toBeNull();
  });

  it("reads enabled providers when no explicit order exists", () => {
    const config = makeAppConfig();
    config.llm.provider_order = [];
    config.llm.preferred_provider = "claude";
    config.llm.providers = [
      makeProviderConfig({ id: "claude" }),
      makeProviderConfig({ id: "gemini", enabled: false }),
      makeProviderConfig({ id: "openai" }),
    ];
    expect(fallbackProvider(config, [], 20)).toBe("openai");
  });
});

describe("healthFor", () => {
  it("matches by provider id and tolerates an unloaded list", () => {
    const rows = [makeProviderHealth({ provider: "openai" })];
    expect(healthFor(rows, "openai")?.provider).toBe("openai");
    expect(healthFor(rows, "claude")).toBeUndefined();
    expect(healthFor(undefined, "claude")).toBeUndefined();
  });
});
