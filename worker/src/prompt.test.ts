import { describe, expect, it } from "vitest";
import type { FocusContext } from "./types";
import { buildSystemPrompt, buildUserMessage } from "./prompt";

const emptyContext: FocusContext = {
  appName: "",
  bundleID: "",
  elementRole: "",
  title: "",
  placeholder: "",
  value: "",
};

describe("buildSystemPrompt", () => {
  it("returns base prompt when context is empty", () => {
    const result = buildSystemPrompt(emptyContext);
    expect(result).toContain("proper punctuation");
    expect(result).not.toContain("<rules>");
  });

  it("appends rules when context is present", () => {
    const ctx: FocusContext = { ...emptyContext, appName: "Slack" };
    const result = buildSystemPrompt(ctx);
    expect(result).toContain("proper punctuation");
    expect(result).toContain("<rules>");
    expect(result).toContain("Chat/messaging apps");
    expect(result).toContain("</rules>");
  });

  it("does not include user context fields", () => {
    const ctx: FocusContext = { ...emptyContext, appName: "MyApp", value: "hey " };
    const result = buildSystemPrompt(ctx);
    expect(result).not.toContain("MyApp");
    expect(result).not.toContain("hey ");
    expect(result).not.toContain("<context>");
  });
});

describe("buildUserMessage", () => {
  it("returns raw transcription when context is empty", () => {
    const result = buildUserMessage("hello world", emptyContext);
    expect(result).toBe("hello world");
  });

  it("wraps transcription and context in XML when context is present", () => {
    const ctx: FocusContext = { ...emptyContext, appName: "Slack" };
    const result = buildUserMessage("hello world", ctx);
    expect(result).toContain("<context>");
    expect(result).toContain("<app>Slack</app>");
    expect(result).toContain("</context>");
    expect(result).toContain("<transcription>hello world</transcription>");
  });

  it("includes all provided fields in XML tags", () => {
    const ctx: FocusContext = {
      appName: "Mail",
      bundleID: "com.apple.mail",
      elementRole: "AXTextArea",
      title: "Message Body",
      placeholder: "Type your message",
      value: "Dear team,",
    };
    const result = buildUserMessage("please review this", ctx);
    expect(result).toContain("<app>Mail</app>");
    expect(result).toContain("<fieldType>AXTextArea</fieldType>");
    expect(result).toContain("<label>Message Body</label>");
    expect(result).toContain("<placeholder>Type your message</placeholder>");
    expect(result).toContain("<existingText>Dear team,</existingText>");
    expect(result).toContain("<transcription>please review this</transcription>");
  });

  it("omits XML tags for empty fields", () => {
    const ctx: FocusContext = { ...emptyContext, appName: "Safari", elementRole: "AXSearchField" };
    const result = buildUserMessage("weather today", ctx);
    expect(result).toContain("<app>Safari</app>");
    expect(result).toContain("<fieldType>AXSearchField</fieldType>");
    expect(result).not.toContain("<label>");
    expect(result).not.toContain("<placeholder>");
    expect(result).not.toContain("<existingText>");
  });

  it("does not include bundleID in output", () => {
    const ctx: FocusContext = { ...emptyContext, appName: "VS Code", bundleID: "com.microsoft.VSCode" };
    const result = buildUserMessage("test", ctx);
    expect(result).not.toContain("com.microsoft.VSCode");
  });
});
