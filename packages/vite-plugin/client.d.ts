declare module "*.xcss" {
  export const styles: Record<string, import("./dist/types.js").ClassName>;
  export const cssVars: Readonly<Record<string, string>>;

  export type Variants<Group extends string> =
    string & { readonly __xcssVariantGroup__?: Group };

}
