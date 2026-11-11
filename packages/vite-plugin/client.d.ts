declare module "*.xcss" {
  export const styles: Record<string, import("./dist/react/class-name.js").ClassName>;
  export const cssVars: Readonly<Record<string, string>>;

  export type Variants<Group extends string> =
    string & { readonly __xcssVariantGroup__?: Group };

}
