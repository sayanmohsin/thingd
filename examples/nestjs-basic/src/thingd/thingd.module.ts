import { Global, Module } from "@nestjs/common";
import { ThingdService } from "./thingd.service";

@Global()
@Module({
  providers: [ThingdService],
  exports: [ThingdService],
})
export class ThingdModule {}
