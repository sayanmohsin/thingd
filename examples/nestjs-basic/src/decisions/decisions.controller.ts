import {
  BadRequestException,
  Body,
  Controller,
  Get,
  NotFoundException,
  Param,
  Post,
} from "@nestjs/common";
// biome-ignore lint/style/useImportType: NestJS constructor injection needs runtime metadata.
import { ThingdService } from "../thingd/thingd.service";

type CreateDecisionBody = {
  id?: string;
  text?: string;
  project?: string;
  confidence?: number;
};

@Controller("decisions")
export class DecisionsController {
  constructor(private readonly thingd: ThingdService) {}

  @Post()
  create(@Body() body: CreateDecisionBody) {
    if (!body.id || !body.text) {
      throw new BadRequestException("id and text are required");
    }

    const decision = this.thingd.put("decisions", {
      id: body.id,
      text: body.text,
      project: body.project ?? "thingd",
      confidence: body.confidence ?? 0.8,
    });

    this.thingd.appendEvent(`project:${decision.project}`, {
      type: "decision.made",
      text: body.text,
      object: `decisions/${decision.id}`,
    });

    const job = this.thingd.pushJob("embed", {
      object: `decisions/${decision.id}`,
    });

    return {
      decision,
      queued: job,
    };
  }

  @Get(":id")
  get(@Param("id") id: string) {
    const decision = this.thingd.get("decisions", id);

    if (!decision) {
      throw new NotFoundException(`Decision ${id} was not found`);
    }

    return decision;
  }
}
