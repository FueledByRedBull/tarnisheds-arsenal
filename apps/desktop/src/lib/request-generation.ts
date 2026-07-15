export interface RequestToken {
  generation: number;
  signature: string;
}

export class LatestRequest {
  private generation = 0;
  private signature = "";

  begin(signature: string): RequestToken {
    this.generation += 1;
    this.signature = signature;
    return { generation: this.generation, signature };
  }

  isCurrent(token: RequestToken): boolean {
    return token.generation === this.generation && token.signature === this.signature;
  }

  invalidate(token: RequestToken): void {
    if (this.isCurrent(token)) {
      this.generation += 1;
      this.signature = "";
    }
  }
}
