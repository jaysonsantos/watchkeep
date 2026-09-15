import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { filterOf, type ListState, listUrl, pageCount, pageOf, sortOf } from "../src/lib/lists.ts";

const state: ListState = { filter: "all", search: "Movie", sort: "title", page: 1, pageSize: 60 };

describe("list state", () => {
  it("builds list URLs without default values", () => {
    assert.equal(listUrl("/movies", state, 2), "/movies?q=Movie&sort=title&page=2");
    assert.equal(
      listUrl("/movies", { ...state, search: "a b", sort: "recent" }, 1, "watched"),
      "/movies?status=watched&q=a%20b",
    );
    assert.equal(listUrl("/shows", { ...state, search: "", sort: "recent" }), "/shows");
  });

  it("clamps the page to the pages that exist", () => {
    assert.equal(pageOf("99", 61), 2, "a page past the end is the last page");
    assert.equal(pageOf("2", 61), 2);
    assert.equal(pageOf(null, 0), 1);
    assert.equal(pageOf("abc", 61), 1);
    assert.equal(pageOf("3", 120, 50), 3);
    assert.equal(pageCount(61, 60), 2);
    assert.equal(pageCount(0, 60), 1);
  });

  it("reads filter and sort values", () => {
    assert.equal(filterOf("watched"), "watched");
    assert.equal(filterOf("x"), "all");
    assert.equal(sortOf("year"), "year");
    assert.equal(sortOf(null), "recent");
  });
});
