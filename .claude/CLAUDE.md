# Project Instructions

<!-- CODEXRAY_INJECTION -->
# Code exploration: use CodeXray MCP tools first

Before any Grep/Glob/Bash for code search, try CodeXray tools first.
They give you AST-verified definitions with signatures and line numbers.

Tool priority (use in this order):
1. codexray_explore("how does X work?") — FIRST for architecture questions
2. codexray_search("what does Y do?")  — FIRST for finding code by behavior
3. codexray_find("Z")                  — FIRST for finding code by name
4. codexray_callers("fn")             — REQUIRED before modifying any function
5. codexray_callees("fn")             — to understand internal dependencies
6. Grep — ONLY for exact strings (error messages, UUIDs, log formats)
7. Glob — ONLY when you already know the exact filename pattern
<!-- /CODEXRAY_INJECTION -->
