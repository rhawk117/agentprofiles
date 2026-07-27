#!/usr/bin/env python3
import ast
import sys
from pathlib import Path

type Hit = tuple[str, int, str, str]

SKIP = {'.git', '.venv', 'venv', '__pycache__', 'node_modules', '.mypy_cache', '.ruff_cache', '.tox', 'build', 'dist'}
USAGE = """usage: pysymbols.py COMMAND [ARGS]

  def NAME [PATH]          where NAME is defined
  refs NAME [PATH]         every reference to NAME
  calls NAME [PATH]        call sites of NAME
  imports PATH             imports declared in PATH
  importers MODULE [PATH]  files that import MODULE
  outline PATH             structure of one file

PATH defaults to the current directory."""


def sources(target: Path) -> list[Path]:
    if target.is_file():
        return [target]
    if not target.is_dir():
        return []
    found = []
    for path in target.rglob('*.py'):
        if any(part in SKIP for part in path.parts):
            continue
        found.append(path)
    return sorted(found)


def parse(path: Path) -> ast.Module | None:
    try:
        return ast.parse(path.read_text(encoding='utf-8', errors='replace'), filename=str(path))
    except (SyntaxError, ValueError, OSError) as exc:
        print(f'{path}: unparsed ({type(exc).__name__})', file=sys.stderr)
        return None


def emit(hits: list[Hit]) -> int:
    for path, line, kind, detail in hits:
        print(f'{path}:{line}: {kind} {detail}'.rstrip())
    if not hits:
        print('no matches')
        return 1
    return 0


def signature(node: ast.FunctionDef | ast.AsyncFunctionDef) -> str:
    args = [a.arg for a in node.args.posonlyargs + node.args.args]
    if node.args.vararg:
        args.append(f'*{node.args.vararg.arg}')
    elif node.args.kwonlyargs:
        args.append('*')
    args.extend(a.arg for a in node.args.kwonlyargs)
    if node.args.kwarg:
        args.append(f'**{node.args.kwarg.arg}')
    return f'{node.name}({", ".join(args)})'


def decorators(node: ast.ClassDef | ast.FunctionDef | ast.AsyncFunctionDef) -> str:
    names = [ast.unparse(d) for d in node.decorator_list]
    if not names:
        return ''
    return ' @' + ' @'.join(names)


def find_defs(name: str, target: Path) -> list[Hit]:
    hits: list[Hit] = []
    for path in sources(target):
        tree = parse(path)
        if tree is None:
            continue
        for node in ast.walk(tree):
            if isinstance(node, ast.ClassDef) and node.name == name:
                bases = ', '.join(ast.unparse(b) for b in node.bases)
                suffix = f'({bases})' if bases else ''
                hits.append((str(path), node.lineno, 'class', f'{name}{suffix}{decorators(node)}'))
            elif isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)) and node.name == name:
                kind = 'async def' if isinstance(node, ast.AsyncFunctionDef) else 'def'
                hits.append((str(path), node.lineno, kind, f'{signature(node)}{decorators(node)}'))
            elif isinstance(node, ast.Assign):
                for goal in node.targets:
                    if isinstance(goal, ast.Name) and goal.id == name:
                        hits.append((str(path), node.lineno, 'assign', name))
            elif isinstance(node, ast.AnnAssign) and isinstance(node.target, ast.Name):
                if node.target.id == name:
                    hits.append((str(path), node.lineno, 'annassign', f'{name}: {ast.unparse(node.annotation)}'))
    return hits


def find_refs(name: str, target: Path, calls_only: bool) -> list[Hit]:
    hits: list[Hit] = []
    for path in sources(target):
        tree = parse(path)
        if tree is None:
            continue
        for node in ast.walk(tree):
            if calls_only:
                if not isinstance(node, ast.Call):
                    continue
                func = node.func
                matched = (isinstance(func, ast.Name) and func.id == name) or (
                    isinstance(func, ast.Attribute) and func.attr == name
                )
                if matched:
                    hits.append((str(path), node.lineno, 'call', ast.unparse(node)[:80]))
                continue
            if isinstance(node, (ast.Import, ast.ImportFrom)):
                for alias in node.names:
                    if (alias.asname or alias.name.split('.')[0]) == name:
                        hits.append((str(path), node.lineno, 'import', ast.unparse(node)[:80]))
            elif isinstance(node, ast.Name) and node.id == name:
                hits.append((str(path), node.lineno, 'name', name))
            elif isinstance(node, ast.Attribute) and node.attr == name:
                hits.append((str(path), node.lineno, 'attr', ast.unparse(node)[:80]))
    return hits


def find_imports(target: Path) -> list[Hit]:
    hits: list[Hit] = []
    for path in sources(target):
        tree = parse(path)
        if tree is None:
            continue
        for node in ast.walk(tree):
            if isinstance(node, ast.Import):
                for alias in node.names:
                    label = f'{alias.name} as {alias.asname}' if alias.asname else alias.name
                    hits.append((str(path), node.lineno, 'import', label))
            elif isinstance(node, ast.ImportFrom):
                module = '.' * node.level + (node.module or '')
                names = ', '.join(a.asname or a.name for a in node.names)
                hits.append((str(path), node.lineno, 'from', f'{module} import {names}'))
    return hits


def find_importers(module: str, target: Path) -> list[Hit]:
    hits: list[Hit] = []
    for path, line, kind, detail in find_imports(target):
        head = detail.split(' import ')[0].split(' as ')[0].strip() if kind == 'from' else detail.split(' as ')[0]
        if head == module or head.startswith(f'{module}.'):
            hits.append((path, line, kind, detail))
    return hits


def outline(path: Path) -> list[Hit]:
    tree = parse(path)
    if tree is None:
        return []
    hits: list[Hit] = []

    def walk(nodes: list[ast.stmt], prefix: str) -> None:
        for node in nodes:
            if isinstance(node, ast.ClassDef):
                bases = ', '.join(ast.unparse(b) for b in node.bases)
                suffix = f'({bases})' if bases else ''
                hits.append((str(path), node.lineno, 'class', f'{prefix}{node.name}{suffix}{decorators(node)}'))
                walk(node.body, f'{prefix}{node.name}.')
            elif isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)):
                kind = 'async def' if isinstance(node, ast.AsyncFunctionDef) else 'def'
                hits.append((str(path), node.lineno, kind, f'{prefix}{signature(node)}{decorators(node)}'))
            elif isinstance(node, ast.Assign) and not prefix:
                for goal in node.targets:
                    if isinstance(goal, ast.Name):
                        hits.append((str(path), node.lineno, 'assign', goal.id))

    walk(tree.body, '')
    return hits


def main(argv: list[str]) -> int:
    if len(argv) < 2:
        print(USAGE, file=sys.stderr)
        return 2
    command = argv[1]
    rest = argv[2:]

    if command == 'outline':
        if not rest:
            print(USAGE, file=sys.stderr)
            return 2
        return emit(outline(Path(rest[0])))
    if command == 'imports':
        if not rest:
            print(USAGE, file=sys.stderr)
            return 2
        return emit(find_imports(Path(rest[0])))
    if command not in {'def', 'refs', 'calls', 'importers'}:
        print(USAGE, file=sys.stderr)
        return 2
    if not rest:
        print(USAGE, file=sys.stderr)
        return 2

    name = rest[0]
    target = Path(rest[1]) if len(rest) > 1 else Path('.')
    if command == 'def':
        return emit(find_defs(name, target))
    if command == 'importers':
        return emit(find_importers(name, target))
    return emit(find_refs(name, target, calls_only=command == 'calls'))


if __name__ == '__main__':
    sys.exit(main(sys.argv))
