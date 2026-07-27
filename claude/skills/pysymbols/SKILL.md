---
name: pysymbols
description: Answers questions about Python code structure from the syntax tree. Use when asked where a symbol is defined, what references or calls it, what a file imports, which files import a module, what a file contains, or whether two parts of a codebase are coupled through imports. Prefer this over grep for any question about Python symbols, call sites, or the import graph.
license: MIT
---

# Python structure queries

`pysymbols.py` answers structural questions about Python code by parsing the abstract syntax tree with the `ast` standard library. Because it reads the tree rather than the text, it never matches a name that appears only in a comment, a docstring, a string literal, or an unrelated language.

Run it from this skill's base directory (`~/.claude/skills/pysymbols/`). It needs no dependencies beyond Python 3.12.

## Commands

```
pysymbols.py def NAME [PATH]          where NAME is defined
pysymbols.py refs NAME [PATH]         every reference to NAME
pysymbols.py calls NAME [PATH]        call sites of NAME
pysymbols.py imports PATH             imports declared in PATH
pysymbols.py importers MODULE [PATH]  files that import MODULE
pysymbols.py outline PATH             structure of one file
```

`PATH` defaults to the current directory. Pass the narrowest path that could contain the answer — the tool walks every `.py` file beneath it, so a package directory returns in milliseconds while a repository root can take seconds.

Output is one result per line as `path:line: kind detail`. Files that fail to parse are reported on stderr and skipped; the walk continues. Exit codes are `0` for a hit, `1` for no matches, `2` for a usage error.

Directories named `.git`, `.venv`, `venv`, `__pycache__`, `node_modules`, `.mypy_cache`, `.ruff_cache`, `.tox`, `build`, and `dist` are skipped.

## Choosing a command

Use `def` to locate a class, function, or module-level assignment. It reports base classes, decorators, and full signatures including keyword-only markers.

Use `refs` for every mention of a name, including the import statement that binds it. Use `calls` when only invocations matter and you want to exclude passing the name as a value.

Use `imports` to see what one file depends on, and `importers` to invert that question. `importers` matches on the module prefix, so `importers redis` catches `import redis.asyncio`.

Use `outline` to understand a file's shape before reading it. Methods are reported qualified as `ClassName.method`, so it doubles as a cheap way to check whether a subclass overrides something.

## Coupling checks

`importers` is the reliable way to test whether two parts of a codebase are independent. If module A is imported by files in area B, the two are coupled regardless of what a file list or a plan claims. This is the check to run before asserting that two sets of files can be edited in parallel, or before changing a signature that might reach outside the files you own.

```
pysymbols.py importers pkg.limits src/
```

## Worked examples

Find a class and its backend:

```
$ pysymbols.py def RateLimiter src/
src/api/limits.py:17: class RateLimiter(Backend) @final
```

Check whether a rename is contained:

```
$ pysymbols.py refs RateLimiter src/api/
src/api/limits.py:17: name RateLimiter
src/api/routes.py:1: import from api.limits import RateLimiter
src/api/routes.py:44: name RateLimiter
```

See a file's shape without reading it:

```
$ pysymbols.py outline src/api/limits.py
src/api/limits.py:5: assign DEFAULT_WINDOW
src/api/limits.py:8: class Backend(Protocol)
src/api/limits.py:12: class RateLimiter
src/api/limits.py:15: def RateLimiter.__init__(self, url, *, window, **opts)
src/api/limits.py:19: async def RateLimiter.check(self, key, cost)
```

## When to reach past this tool

The six commands cover symbol lookup, references, imports, and file structure. For anything else — which functions carry a given decorator, where an attribute is assigned, which `except` handlers swallow the exception, how deep a call chain runs — write a short throwaway query against `ast` directly rather than reaching for a regex. Load the module, walk the tree, report `file:line`, and quote the command you ran so the result can be reproduced.

For literal strings, configuration values, non-Python files, or raw file contents, use `grep` instead. Those are not in the syntax tree.

## Limits worth stating in any report

`ast` reads source, not runtime. It cannot resolve dynamic dispatch, `getattr` lookups, monkeypatching, conditional imports, or anything constructed at run time. A conclusion drawn only from the syntax tree is an inference, not a verified fact — when a question turns on runtime behavior, run the code rather than reading the tree.
