<template>
    <v-overlay
        :model-value="showAdder"
        @click:outside="$emit('update:showAdder')"
        @update="$emit('update:showAdder')"
        class="align-center justify-center"
    >
        <div style="max-width: 600px" class="align-center justify-center">
            <v-btn
                v-for="strat in availableStrategies"
                class="ma-1"
                @click="addNewStrategy(strat)"
            >
                {{ strat.name }}
            </v-btn>
        </div>
    </v-overlay>
</template>

<script>
export default {
    name: 'AddStrategy',
    components: {},
    props: {
        showAdder: Boolean,
        availableStrategies: Array
    },
    emits: ['strategyAdded', 'update:showAdder'],
    data() {
        return {}
    },
    mounted() {
    },
    methods: {
        addNewStrategy(strat) {
            this.$emit('strategyAdded', {
                "key": strat.key,
                "name": strat.name,
                "averageTime": strat.averageTime,
                "probability": strat.probability,
                "series": 1,
                "strategies": []
            });

            this.$emit('update:showAdder', false);
        }
    }
}
</script>
