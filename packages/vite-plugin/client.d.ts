declare module "*.xcss" {
  const styles: Record<string, string>;

  export type Variants<Group extends string> =
    string & { readonly __xcssVariantGroup__?: Group };

  export default styles;
}
