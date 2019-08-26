export interface IntrospectionResultData {
  __schema: {
    types: {
      kind: string;
      name: string;
      possibleTypes: {
        name: string;
      }[];
    }[];
  };
}

const result: IntrospectionResultData = {
  __schema: {
    types: [
      {
        kind: "INTERFACE",
        name: "Component",
        possibleTypes: [
          {
            name: "ServerComponent",
          },
          {
            name: "OperatingSystemComponent",
          },
        ],
      },
    ],
  },
};

export default result;
