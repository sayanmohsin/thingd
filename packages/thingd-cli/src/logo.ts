const orange = (s: string) => `\x1b[38;2;224;83;22m${s}\x1b[0m`;
const cyan = (s: string) => `\x1b[38;2;0;196;212m${s}\x1b[0m`;

export function logoText(): string {
  return `${orange("{")}${cyan("t")}${cyan("h")}${cyan("i")}${cyan("n")}${cyan("g")}${orange(":")}${orange("d")}${orange("}")}`;
}

export function logoLine(): string {
  return `${logoText()}\n`;
}
