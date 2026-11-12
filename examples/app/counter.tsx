import { mount, signal, Text, View, TapGesture, LongPressGesture, PanGesture } from "bubble";

function Counter() {
  const count = signal(0);
  const gesture = signal("idle");
  return (
    <View backgroundColor="oklch(0.2 0.01 264)" flexGrow={1} flexShrink={1} padding={[12, 12, 12, 12]} gap={8}>
      <Text color="#ffffff" fontSize={24} lineHeight={30} textAlign="center" fontWeight={700}>Count: {count()}</Text>
      <View
        backgroundColor={0xff3b82f6}
        radius={8}
        padding={[12, 8, 8, 12]}
      >
        <TapGesture onTap={() => count.update((c) => c + 1)} />
        <Text color="#ffffff" fontSize={16}>Increment</Text>
      </View>
      <View overflowY="scroll" flexGrow={1} flexShrink={1}>
        <Text color="#dddddd" fontSize={18}>Alpha</Text>
        <Text color="#dddddd" fontSize={18}>Bravo</Text>
        <Text color="#dddddd" fontSize={18}>Charlie</Text>
        <Text color="#dddddd" fontSize={18}>Delta</Text>
        <Text color="#dddddd" fontSize={18}>Echo</Text>
        <Text color="#dddddd" fontSize={18}>Foxtrot</Text>
        <Text color="#dddddd" fontSize={18}>Golf</Text>
        <Text color="#dddddd" fontSize={18}>Hotel</Text>
        <Text color="#dddddd" fontSize={18}>India</Text>
        <Text color="#dddddd" fontSize={18}>Juliet</Text>
        <Text color="#dddddd" fontSize={18}>Kilo</Text>
        <Text color="#dddddd" fontSize={18}>Lima</Text>
        <Text color="#dddddd" fontSize={18}>Item 13</Text>
        <Text color="#dddddd" fontSize={18}>Item 14</Text>
        <Text color="#dddddd" fontSize={18}>Item 15</Text>
        <Text color="#dddddd" fontSize={18}>Item 16</Text>
        <Text color="#dddddd" fontSize={18}>Item 17</Text>
        <Text color="#dddddd" fontSize={18}>Item 18</Text>
        <Text color="#dddddd" fontSize={18}>Item 19</Text>
        <Text color="#dddddd" fontSize={18}>Item 20</Text>
        <Text color="#dddddd" fontSize={18}>Item 21</Text>
        <Text color="#dddddd" fontSize={18}>Item 22</Text>
        <Text color="#dddddd" fontSize={18}>Item 23</Text>
        <Text color="#dddddd" fontSize={18}>Item 24</Text>
        <Text color="#dddddd" fontSize={18}>Item 25</Text>
        <Text color="#dddddd" fontSize={18}>Item 26</Text>
        <Text color="#dddddd" fontSize={18}>Item 27</Text>
        <Text color="#dddddd" fontSize={18}>Item 28</Text>
        <Text color="#dddddd" fontSize={18}>Item 29</Text>
        <Text color="#dddddd" fontSize={18}>Item 30</Text>
        <Text color="#dddddd" fontSize={18}>Item 31</Text>
        <Text color="#dddddd" fontSize={18}>Item 32</Text>
        <Text color="#dddddd" fontSize={18}>Item 33</Text>
        <Text color="#dddddd" fontSize={18}>Item 34</Text>
        <Text color="#dddddd" fontSize={18}>Item 35</Text>
        <Text color="#dddddd" fontSize={18}>Item 36</Text>
        <Text color="#dddddd" fontSize={18}>Item 37</Text>
        <Text color="#dddddd" fontSize={18}>Item 38</Text>
        <Text color="#dddddd" fontSize={18}>Item 39</Text>
        <Text color="#dddddd" fontSize={18}>Item 40</Text>
        <Text color="#dddddd" fontSize={18}>Item 41</Text>
        <Text color="#dddddd" fontSize={18}>Item 42</Text>
        <Text color="#dddddd" fontSize={18}>Item 43</Text>
        <Text color="#dddddd" fontSize={18}>Item 44</Text>
        <Text color="#dddddd" fontSize={18}>Item 45</Text>
        <Text color="#dddddd" fontSize={18}>Item 46</Text>
        <Text color="#dddddd" fontSize={18}>Item 47</Text>
        <Text color="#dddddd" fontSize={18}>Item 48</Text>
      </View>
      <View height={24} flexShrink={0} backgroundColor={0xff222222} radius={6}>
        <PanGesture
          axis="x"
          minDistance={8}
          onPanStart={() => gesture.update(() => "pan")}
          onPanUpdate={(event) => gesture.update(() => "pan " + Math.round(event.translationX))}
          onPanEnd={() => gesture.update(() => "pan end")}
        />
        <TapGesture onTap={() => gesture.update(() => "tap")} />
        <LongPressGesture duration={400} onLongPress={() => gesture.update(() => "long")} />
        <Text fontSize={14} color="#ffffff">{gesture()}</Text>
      </View>
    </View>
  );
}

mount(Counter);
