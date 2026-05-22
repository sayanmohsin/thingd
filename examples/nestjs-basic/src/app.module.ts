import { Module } from "@nestjs/common";
import { DecisionsController } from "./decisions/decisions.controller";
import { JobsController } from "./jobs/jobs.controller";
import { ThingdModule } from "./thingd/thingd.module";

@Module({
  imports: [ThingdModule],
  controllers: [DecisionsController, JobsController],
})
export class AppModule {}
