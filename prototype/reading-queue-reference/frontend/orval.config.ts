import { defineConfig } from 'orval';

const generatedApiRoot =
  process.env.YYDRA_GENERATED_API_ROOT ?? './src/generated/public-api';
const openApiInput = process.env.YYDRA_OPENAPI_INPUT ?? '../contracts/openapi.json';

export default defineConfig({
  publicApi: {
    input: openApiInput,
    output: {
      target: `${generatedApiRoot}/client.ts`,
      schemas: {
        path: `${generatedApiRoot}/model`,
        type: 'zod',
      },
      client: 'fetch',
      clean: true,
      override: {
        includeZodSchemaInArguments: true,
        mutator: {
          path: './src/framework/api/fetcher.ts',
          name: 'frameworkFetch',
        },
        fetch: {
          includeHttpResponseReturnType: true,
          runtimeValidation: true,
        },
      },
    },
  },
});
