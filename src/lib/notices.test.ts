import { describe, expect, it } from "vitest";
import { unwrapNotices } from "./notices";

describe("unwrapNotices", () => {
  it("joins the wrapped lines of a paragraph, keeps headings, lists, links and separators", () => {
    const text = [
      "-----------------------------------------------------------",
      "PREAMBLE",
      "The goals of the Open Font License (OFL) are to stimulate worldwide",
      "development of collaborative font projects, to support the font creation",
      "efforts of academic communities, named in the",
      "copyright statement(s).",
      "",
      "This license is copied below, and is also available with a FAQ at:",
      "http://scripts.sil.org/OFL",
      "",
      "1) Neither the Font Software nor any of its individual components,",
      "in Original or Modified Versions, may be sold by itself.",
      "2) Original or Modified Versions of the Font Software may be bundled,",
      "redistributed and/or sold with any software.",
      "",
      "Copyright (c) 2015 Example Company and other contributors of the project",
      "Copyright (c) 2016 Another Example Person",
      "      (a) You must give any other recipients of the Work or Derivative",
      "          Works a copy of this License; and",
      "",
      "DISCLAIMER",
      "THE FONT SOFTWARE IS PROVIDED \"AS IS\", WITHOUT WARRANTY OF ANY KIND,",
      "EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO ANY WARRANTIES OF",
      "MERCHANTABILITY.",
      "==============================================================================",
      "Used by: example 1.0.0",
    ].join("\r\n");
    expect(unwrapNotices(text).split("\n")).toEqual([
      "-----------------------------------------------------------",
      "PREAMBLE",
      "The goals of the Open Font License (OFL) are to stimulate worldwide development of collaborative font projects, to support the font creation efforts of academic communities, named in the copyright statement(s).",
      "",
      "This license is copied below, and is also available with a FAQ at:",
      "http://scripts.sil.org/OFL",
      "",
      "1) Neither the Font Software nor any of its individual components, in Original or Modified Versions, may be sold by itself.",
      "2) Original or Modified Versions of the Font Software may be bundled, redistributed and/or sold with any software.",
      "",
      "Copyright (c) 2015 Example Company and other contributors of the project",
      "Copyright (c) 2016 Another Example Person",
      "      (a) You must give any other recipients of the Work or Derivative Works a copy of this License; and",
      "",
      "DISCLAIMER",
      "THE FONT SOFTWARE IS PROVIDED \"AS IS\", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO ANY WARRANTIES OF MERCHANTABILITY.",
      "==============================================================================",
      "Used by: example 1.0.0",
    ]);
  });
});
