# Reusable review prompt after each phase

```text
Now review the work you just completed.

Do the following:
1. inspect for architectural drift, unnecessary complexity, dead code, and tight coupling
2. identify weak points, missing tests, and unstable assumptions
3. fix the most important issues without rewriting stable code
4. improve documentation for the new feature
5. run format/lint/build/test if available
6. give me a concise summary of:
   - what is complete
   - what is partially complete
   - what still needs to be built next
```
