import { Global, Module } from "@nestjs/common";
import { MemorydService } from "./memoryd.service";

@Global()
@Module({
  providers: [MemorydService],
  exports: [MemorydService],
})
export class MemorydModule {}
