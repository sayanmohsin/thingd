import { Module } from "@nestjs/common";
import { DecisionsController } from "./decisions/decisions.controller";
import { JobsController } from "./jobs/jobs.controller";
import { MemorydModule } from "./memoryd/memoryd.module";

@Module({
  imports: [MemorydModule],
  controllers: [DecisionsController, JobsController],
})
export class AppModule {}
