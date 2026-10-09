import { LlmProvider } from '@forklaunch/interfaces-mlse/interfaces';
import {
  EmbedRequestDto,
  EmbedResponseDto,
  GenerateRequestDto,
  GenerateResponseDto,
  LlmProviderDescriptorDto
} from '@forklaunch/interfaces-mlse/types';

// Common class for AI providers. The dependency container checks singletons
// with instanceof, so a deployment can choose its provider at startup only
// if every provider shares a class. It is not abstract because the
// container's type slot only accepts constructible classes; every method must
// be overridden.
export class LlmProviderBase implements LlmProvider {
  // eslint-disable-next-line @typescript-eslint/no-unused-vars
  generate(_request: GenerateRequestDto): Promise<GenerateResponseDto> {
    return Promise.reject(new Error(`${this.constructor.name} does not implement generate`));
  }

  // eslint-disable-next-line @typescript-eslint/no-unused-vars
  embed(_request: EmbedRequestDto): Promise<EmbedResponseDto> {
    return Promise.reject(new Error(`${this.constructor.name} does not implement embed`));
  }

  describe(): LlmProviderDescriptorDto {
    throw new Error(`${this.constructor.name} does not implement describe`);
  }
}
