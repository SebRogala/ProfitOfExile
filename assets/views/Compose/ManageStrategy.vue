<template>
    <v-card
        class="mb-1"
        variant="elevated"
    >
        <v-card-title>
            {{ value.name }}
        </v-card-title>

        <v-card-text>
            <v-text-field
                class="mb-5"
                label="Run times"
                variant="outlined"
                density="compact"
                hide-details
                v-model="value.series"
            ></v-text-field>
            <v-text-field
                class="mb-5"
                label="Average time to run (seconds)"
                variant="outlined"
                density="compact"
                hide-details
                v-model="value.averageTime"
            ></v-text-field>
            <v-text-field
                label="Probability"
                variant="outlined"
                density="compact"
                hide-details
                v-model="value.probability"
            ></v-text-field>

            <p v-if="value.strategies.length" class="text-h6 mt-3">Composed strategies:</p>

            <manage-strategy
                class="mt-3 ml-10"
                v-for="(strat, key) in value.strategies"
                :value="strat"
                @deleted="value.strategies.splice(key, 1)"
            ></manage-strategy>

        </v-card-text>

        <v-card-actions>
            <v-btn
                color="error"
                @click="$emit('deleted')"
            >Remove
            </v-btn>
        </v-card-actions>
    </v-card>
</template>

<script>
export default {
    name: 'ManageStrategy',
    emits: ['deleted'],
    props: {
        value: Object
    },
    data() {
        return {
        }
    },
    mounted() {
    },
    methods: {
    }
}
</script>
