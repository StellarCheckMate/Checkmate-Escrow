# Checkmate-Escrow Frontend

React + TypeScript + Vite frontend for the Checkmate-Escrow platform.

## Local Development

### Prerequisites

- Node.js 18+
- npm

### Setup

```bash
cd frontend
npm install
```

### Running the dev server

```bash
npm run dev
```

The dev server starts on `http://localhost:5173` by default.

#### API proxy (CORS)

During local development, requests to `/api` are automatically proxied to the event-indexer at `http://localhost:8080`, avoiding CORS errors. Start the event-indexer on port 8080 before running the frontend.

### Running tests

```bash
npm test
```

### Updating Snapshots

When component output changes intentionally (e.g., updating UI text, modifying component structure), snapshot tests will fail. To update snapshots after verifying the changes are correct:

```bash
npm test -- --update-snapshots
```

Or update snapshots in watch mode:

```bash
npm run test:watch -- --update-snapshots
```

**When to update snapshots:**
- ✅ You've intentionally changed component output (markup, text, styling)
- ✅ You've reviewed the snapshot diff and confirmed the changes are correct
- ✅ The change is part of a planned feature or refactor

**When NOT to update snapshots:**
- ❌ A test is failing and you haven't made any intentional changes
- ❌ You haven't reviewed what changed in the snapshot
- ❌ The failure might indicate a bug or unintended side effect

**Tip:** Always review snapshot diffs carefully. A failing snapshot test is often catching a real issue. Only update snapshots when you're certain the new output is correct.

### Building for production

```bash
npm run build
```

---

This template provides a minimal setup to get React working in Vite with HMR and some ESLint rules.

Currently, two official plugins are available:

- [@vitejs/plugin-react](https://github.com/vitejs/vite-plugin-react/blob/main/packages/plugin-react) uses [Oxc](https://oxc.rs)
- [@vitejs/plugin-react-swc](https://github.com/vitejs/vite-plugin-react/blob/main/packages/plugin-react-swc) uses [SWC](https://swc.rs/)

## React Compiler

The React Compiler is not enabled on this template because of its impact on dev & build performances. To add it, see [this documentation](https://react.dev/learn/react-compiler/installation).

## Expanding the ESLint configuration

If you are developing a production application, we recommend updating the configuration to enable type-aware lint rules:

```js
export default defineConfig([
  globalIgnores(['dist']),
  {
    files: ['**/*.{ts,tsx}'],
    extends: [
      // Other configs...

      // Remove tseslint.configs.recommended and replace with this
      tseslint.configs.recommendedTypeChecked,
      // Alternatively, use this for stricter rules
      tseslint.configs.strictTypeChecked,
      // Optionally, add this for stylistic rules
      tseslint.configs.stylisticTypeChecked,

      // Other configs...
    ],
    languageOptions: {
      parserOptions: {
        project: ['./tsconfig.node.json', './tsconfig.app.json'],
        tsconfigRootDir: import.meta.dirname,
      },
      // other options...
    },
  },
])
```

You can also install [eslint-plugin-react-x](https://github.com/Rel1cx/eslint-react/tree/main/packages/plugins/eslint-plugin-react-x) and [eslint-plugin-react-dom](https://github.com/Rel1cx/eslint-react/tree/main/packages/plugins/eslint-plugin-react-dom) for React-specific lint rules:

```js
// eslint.config.js
import reactX from 'eslint-plugin-react-x'
import reactDom from 'eslint-plugin-react-dom'

export default defineConfig([
  globalIgnores(['dist']),
  {
    files: ['**/*.{ts,tsx}'],
    extends: [
      // Other configs...
      // Enable lint rules for React
      reactX.configs['recommended-typescript'],
      // Enable lint rules for React DOM
      reactDom.configs.recommended,
    ],
    languageOptions: {
      parserOptions: {
        project: ['./tsconfig.node.json', './tsconfig.app.json'],
        tsconfigRootDir: import.meta.dirname,
      },
      // other options...
    },
  },
])
```
