<View background={0xff111111} padding={[16, 16, 16, 16]}>
  <Text fontSize={28} color={0xffffffff} padding={[0, 0, 0, 8]}>Count: 0</Text>
  <View handler={1} background={0xff3b82f6} radius={8} padding={[16, 10, 16, 10]}>
    <Text fontSize={18} color={0xffffffff}>Increment</Text>
  </View>
  <View scrollY flexGrow={1} flexShrink={1} padding={[0, 12, 0, 0]}>
    <View>
      <Repeat count={40}>
        <Text fontSize={18} color={0xffdddddd} padding={[0, 6, 0, 6]}>Row {index}</Text>
      </Repeat>
    </View>
  </View>
</View>
