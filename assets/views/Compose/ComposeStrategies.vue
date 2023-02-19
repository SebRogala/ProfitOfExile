<template>
    <!--{{availableStrategies}}-->
    <v-card
        class="mb-1"
        variant="outlined"
        v-for="(strat, key) in composedStrategies">

        <v-card-title>
            {{ strat.name }}
        </v-card-title>

        <v-card-text>
            <v-text-field
                class="mb-5"
                label="Run times"
                variant="outlined"
                density="compact"
                hide-details
                v-model="strat.series"
            ></v-text-field>
            <v-text-field
                class="mb-5"
                label="Average time to run"
                variant="outlined"
                density="compact"
                hide-details
                v-model="strat.averageTime"
            ></v-text-field>
            <v-text-field
                label="Probability"
                variant="outlined"
                density="compact"
                hide-details
                v-model="strat.probability"
            ></v-text-field>
        </v-card-text>

        <v-card-actions>
            <v-btn
                color="error"
                @click="composedStrategies.splice(key, 1)"
            >Remove
            </v-btn>
        </v-card-actions>
    </v-card>

    <v-btn
        color="success"
        icon="mdi-plus"
        @click="addStrategyOverlay = !addStrategyOverlay"
    >
    </v-btn>

    <v-overlay v-model="addStrategyOverlay" contained class="align-center justify-center">
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

    <pre>{{ composedStrategies }}</pre>
</template>

<script>
export default {
    name: 'ComposeStrategies',
    data() {
        return {
            addStrategyOverlay: false,
            availableStrategies: [],
            composedStrategies: [
                {
                    "key": "run-shaper",
                    "name": "RunShaper",
                    "averageTime": 480,
                    "probability": 100,
                    "series": 1,
                    "strategies": []
                }
            ]
        }
    },
    mounted() {
        this.loadData();
    },
    methods: {
        loadData() {
            this.$api.get('/strategy/get-all').then(res => {
                this.availableStrategies = res.data;
            });
        },
        addNewStrategy(strat) {
            this.composedStrategies.push({
                "key": strat.key,
                "name": strat.name,
                "averageTime": strat.averageTime,
                "probability": strat.probability,
                "series": 1,
                "strategies": []
            });
            this.addStrategyOverlay = false;
        }
    }
}
</script>
