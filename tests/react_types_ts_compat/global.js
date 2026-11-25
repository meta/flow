declare const html: React.DetailedReactHTMLElement<{readonly role: string}, HTMLDivElement>;
html as React.DetailedReactHTMLElement<{readonly role?: string}, HTMLElement>; // ok
html as React.DetailedReactHTMLElement<{readonly role: string}, HTMLInputElement>; // error
