# Plan wdrożenia

Tekst z **pogrubieniem**, `kodem` i [linkiem](https://example.com).

```mermaid
graph LR
  A[Commit] --> B{CI}
  B -- zielone --> C[Release]
  B -- czerwone --> D[Poprawka]
  D --> A
```

| Platforma | Paczka |
|---|---|
| macOS | .dmg |
| Linux | .deb, .AppImage |
| Windows | setup.exe |

- [x] testy
- [ ] podpis
