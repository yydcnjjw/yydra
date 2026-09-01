import { defineConfig } from 'orval';

const generatedApiRoot =
  process.env.YYDRA_GENERATED_API_ROOT ?? './src/generated/public-api';
const openApiInput = process.env.YYDRA_OPENAPI_INPUT ?? '../contracts/openapi.json';

export default defineConfig({
  publicApi: {
    input: openApiInput,
    output: {
      target: `${generatedApiRoot}/client.ts`,
      schemas: `${generatedApiRoot}/model`,
      client: 'fetch',
      clean: true,
    },
  },
});
