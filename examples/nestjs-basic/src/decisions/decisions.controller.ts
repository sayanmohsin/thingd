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
import { MemorydService } from "../memoryd/memoryd.service";

type CreateDecisionBody = {
  id?: string;
  text?: string;
  project?: string;
  confidence?: number;
};

@Controller("decisions")
export class DecisionsController {
  constructor(private readonly memoryd: MemorydService) {}

  @Post()
  create(@Body() body: CreateDecisionBody) {
    if (!body.id || !body.text) {
      throw new BadRequestException("id and text are required");
    }

    const decision = this.memoryd.put("decisions", {
      id: body.id,
      text: body.text,
      project: body.project ?? "memoryd",
      confidence: body.confidence ?? 0.8,
    });

    this.memoryd.appendEvent(`project:${decision.project}`, {
      type: "decision.made",
      text: body.text,
      object: `decisions/${decision.id}`,
    });

    const job = this.memoryd.pushJob("embed", {
      object: `decisions/${decision.id}`,
    });

    return {
      decision,
      queued: job,
    };
  }

  @Get(":id")
  get(@Param("id") id: string) {
    const decision = this.memoryd.get("decisions", id);

    if (!decision) {
      throw new NotFoundException(`Decision ${id} was not found`);
    }

    return decision;
  }
}
