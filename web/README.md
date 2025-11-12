# OpsBox Frontend

SvelteKit-based frontend with modular architecture, built with Svelte 5 and TypeScript.

## Architecture

The frontend uses a modular architecture with clear separation of concerns:

- **Types** (`src/lib/modules/logseek/types/`): Centralized TypeScript definitions
- **API Clients** (`src/lib/modules/logseek/api/`): Backend API encapsulation
- **Utils** (`src/lib/modules/logseek/utils/`): Reusable utility functions
- **Composables** (`src/lib/modules/logseek/composables/`): Svelte 5 Runes state management
- **Components** (`src/lib/modules/logseek/components/`): Reusable UI components (future)

See `docs/FRONTEND_DEVELOPMENT.md` for detailed development guide.

## Developing

Once you've created a project and installed dependencies with `npm install` (or `pnpm install` or `yarn`), start a development server:

```sh
pnpm dev

# or start the server and open the app in a new browser tab
pnpm dev -- --open
```

## Building

To create a production version of your app:

```sh
pnpm build
```

You can preview the production build with `pnpm preview`.

> To deploy your app, you may need to install an [adapter](https://svelte.dev/docs/kit/adapters) for your target environment.
